//! The DupeLens app as a library, so that one body of code serves both hosts:
//! - desktop: `main.rs` opens the database, then calls [`open_window`];
//! - Android: the system loads this library and calls [`run`] (Tauri's mobile entry point;
//!   there is no `main` on a phone). See docs/decisions/0016-android-app.md.
//!
//! For the window this file is only a host. It maps Tauri's mechanisms onto the shared
//! [`Service`](dupelens_core::service::Service):
//! - the `api(cmd, args)` command forwards to `Service::call` (plus the host commands
//!   `pick_folder`, `suggested_folders` and `quit`);
//! - the `thumb` URI scheme serves `Service::thumbnail`.
//!
//! The page reaches this host only through `ui/js/api.js`. See docs/UI.md.

use dupelens_core::service::{ApiError, Service};
use dupelens_core::Engine;
use serde_json::{json, Value};
use tauri::{http, Manager, State};

/// `async` so it runs off the main thread: scans, comparisons and the folder dialog must
/// not freeze the window.
#[tauri::command(async)]
fn api(
    app: tauri::AppHandle,
    service: State<'_, Service>,
    cmd: String,
    args: Value,
) -> Result<Value, ApiError> {
    match cmd.as_str() {
        "pick_folder" => pick_folder(&app),
        // The user's standard folders that exist, offered as one-click choices on first run.
        // Only these well-known locations are listed; nothing is read from them.
        "suggested_folders" => {
            let paths = app.path();
            let mut candidates = vec![
                ("Pictures", paths.picture_dir()),
                ("Downloads", paths.download_dir()),
            ];
            // A phone has no Desktop folder (Tauri only offers the lookup on desktop).
            #[cfg(desktop)]
            candidates.push(("Desktop", paths.desktop_dir()));
            candidates.extend([
                ("Documents", paths.document_dir()),
                ("Videos", paths.video_dir()),
                ("Music", paths.audio_dir()),
            ]);
            let found: Vec<Value> = candidates
                .into_iter()
                .filter_map(|(name, dir)| {
                    let dir = dir.ok().filter(|d| d.is_dir())?;
                    Some(json!({ "name": name, "path": dir.to_string_lossy() }))
                })
                .collect();
            Ok(json!(found))
        }
        "quit" => {
            app.exit(0);
            Ok(Value::Null)
        }
        _ => service.call(&cmd, args),
    }
}

#[cfg(desktop)]
fn pick_folder(app: &tauri::AppHandle) -> Result<Value, ApiError> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .set_title("Choose a folder to scan")
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok());
    Ok(json!(picked.map(|p| p.to_string_lossy().into_owned())))
}

// Android has no folder picker that hands back a plain path (its picker returns content://
// addresses). Phone folders come from the suggested list until the plugin of decision 0016
// exists; picking nothing is the honest answer meanwhile.
#[cfg(mobile)]
fn pick_folder(_app: &tauri::AppHandle) -> Result<Value, ApiError> {
    Ok(Value::Null)
}

/// Everything both hosts share: plugins, the `api` command and the `thumb` image scheme.
fn builder() -> tauri::Builder<tauri::Wry> {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_dialog::init());
    builder
        .invoke_handler(tauri::generate_handler![api])
        .register_asynchronous_uri_scheme_protocol("thumb", |ctx, request, responder| {
            let service = ctx.app_handle().state::<Service>().inner().clone();
            // Decoding a photo takes a while; keep it off the webview's thread.
            std::thread::spawn(move || {
                let uri = request.uri();
                let id = uri.path().trim_start_matches('/').parse::<i64>().ok();
                let large = uri.query().is_some_and(|q| q.contains("size=large"));
                let response = match id.and_then(|id| service.thumbnail(id, large).ok()) {
                    Some(jpeg) => http::Response::builder()
                        .header("Content-Type", "image/jpeg")
                        .header("Cache-Control", "private, max-age=600")
                        .body(jpeg.to_vec()),
                    None => http::Response::builder().status(404).body(Vec::new()),
                };
                // The builder only fails on invalid headers, which these aren't; if it ever
                // does, answer with a bare 500 rather than panicking the worker thread.
                responder.respond(response.unwrap_or_else(|_| {
                    let mut failed = http::Response::new(Vec::new());
                    *failed.status_mut() = http::StatusCode::INTERNAL_SERVER_ERROR;
                    failed
                }));
            });
        })
}

/// Desktop: opens the window on an engine that `main.rs` already opened.
pub fn open_window(engine: Engine) -> anyhow::Result<()> {
    builder()
        .manage(Service::new(engine))
        .run(tauri::generate_context!())
        .map_err(|e| anyhow::anyhow!("the DupeLens window failed: {e}"))
}

/// Android entry point. The database lives in the app's private data folder, which Android
/// creates per app and which needs no permission.
#[cfg(mobile)]
#[tauri::mobile_entry_point]
pub fn run() {
    builder()
        .setup(|app| {
            let db = app.path().app_data_dir()?.join("dupelens.db");
            // The lock only guards against a second process on the same database; the
            // phone runs one instance, but keeping it costs nothing and keeps one code path.
            let (engine, lock) = dupelens_core::cli::open_engine(Some(db))?;
            app.manage(lock);
            app.manage(Service::new(engine));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("the DupeLens app failed to start");
}
