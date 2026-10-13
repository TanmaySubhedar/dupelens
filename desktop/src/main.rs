//! `dupelens`: the DupeLens app. Started without a command (e.g. double-clicked) it opens the
//! window; `dupelens <command>` in a terminal runs the CLI commands (`scan`, `groups`,
//! `clean`, ...) instead, from `dupelens_core::cli`.
//!
//! For the window this file is only a host. It maps Tauri's mechanisms onto the shared
//! [`Service`](dupelens_core::service::Service):
//! - the `api(cmd, args)` command forwards to `Service::call` (plus the host commands
//!   `pick_folder` and `quit`);
//! - the `thumb` URI scheme serves `Service::thumbnail`.
//!
//! The page reaches this host only through `ui/js/api.js`. See docs/UI.md.

// Deliberately a console program on Windows (no `windows_subsystem = "windows"`): only then
// does a terminal wait for `dupelens scan` and show its output, and can `clean` ask for
// confirmation. When double-clicked, `hide_own_console` closes the console Windows opened.

use clap::Parser;
use dupelens_core::cli;
use dupelens_core::service::{ApiError, Service};
use dupelens_core::Engine;
use std::path::PathBuf;
use serde_json::{json, Value};
use tauri::{http, Manager, State};
use tauri_plugin_dialog::DialogExt;

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
        "pick_folder" => {
            let picked = app
                .dialog()
                .file()
                .set_title("Choose a folder to scan")
                .blocking_pick_folder()
                .and_then(|p| p.into_path().ok());
            Ok(json!(picked.map(|p| p.to_string_lossy().into_owned())))
        }
        "quit" => {
            app.exit(0);
            Ok(Value::Null)
        }
        _ => service.call(&cmd, args),
    }
}

#[derive(Parser)]
#[command(
    name = "dupelens",
    version,
    about = "Find and clean up duplicate photos, entirely offline. Without a command, opens the app."
)]
struct Args {
    /// Database file (defaults to the per-user app data folder).
    #[arg(long, global = true, env = "DUPELENS_DB")]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<cli::Command>,
}

fn main() {
    let args = Args::parse();
    let engine = match cli::open_engine(args.db) {
        Ok(engine) => engine,
        Err(e) => fail(e),
    };
    let Some(command) = args.command else {
        hide_own_console();
        return open_window(engine);
    };
    let mut engine = engine;
    if let Err(e) = cli::run(&mut engine, command) {
        fail(e);
    }
}

fn fail(e: anyhow::Error) -> ! {
    eprintln!("Error: {e:#}");
    std::process::exit(1)
}

/// Double-clicking a console program makes Windows open a console just for it. If nobody else
/// is attached to ours (no terminal started us), close it so only the app window remains.
#[cfg(windows)]
fn hide_own_console() {
    use windows_sys::Win32::System::Console::{FreeConsole, GetConsoleProcessList};
    let mut pids = [0u32; 2];
    // SAFETY: the buffer is valid for `pids.len()` entries.
    if unsafe { GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) } == 1 {
        // SAFETY: no arguments; we never use the console after this.
        unsafe { FreeConsole() };
    }
}

#[cfg(not(windows))]
fn hide_own_console() {}

fn open_window(engine: Engine) {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Service::new(engine))
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
                responder.respond(response.expect("valid response"));
            });
        })
        .run(tauri::generate_context!())
        .expect("error while running DupeLens");
}
