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

use dupelens_core::service::{ApiError, ErrorKind, Service};
use dupelens_core::Engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::{http, Manager, State};
use tauri_plugin_dupelens_device::DeviceExt;

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
        "suggested_folders" => Ok(json!(suggested_folders(&app))),
        // Android only: may the app read the phone's files, and the page to allow it.
        "storage_status" => app
            .device()
            .storage_status()
            .map(|s| json!(s))
            .map_err(|m| ApiError::new(ErrorKind::Failed, m)),
        "open_storage_settings" => app
            .device()
            .open_storage_settings()
            .map(|()| Value::Null)
            .map_err(|m| ApiError::new(ErrorKind::Failed, m)),
        // The in-app folder browser (a phone has no native dialog that returns paths).
        "browse_folder" => browse_folder(args.get("path").and_then(Value::as_str)),
        "quit" => {
            app.exit(0);
            Ok(Value::Null)
        }
        _ => service.call(&cmd, args),
    }
}

/// The top of what the in-app folder browser may show: the phone's shared storage on Android,
/// the home folder elsewhere.
fn browse_root() -> Option<PathBuf> {
    if cfg!(target_os = "android") {
        // Android sets EXTERNAL_STORAGE for every app; "/storage/emulated/0" is the usual value.
        Some(PathBuf::from(
            std::env::var("EXTERNAL_STORAGE").unwrap_or_else(|_| "/storage/emulated/0".into()),
        ))
    } else {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
    }
}

/// True when `here` is `root` or below it and contains no `..` step. Because no `..` is
/// allowed, the lexical parent of an accepted path is also inside (or equal to) the root.
fn is_inside_root(here: &Path, root: &Path) -> bool {
    here.starts_with(root)
        && !here
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Lists the sub-folders of `path` (or of the root), for the in-app folder browser. Only
/// names are read, nothing inside the files; hidden folders (starting with a dot) are left out.
fn browse_folder(path: Option<&str>) -> Result<Value, ApiError> {
    let root = browse_root()
        .ok_or_else(|| ApiError::new(ErrorKind::Failed, "cannot find the storage folder"))?;
    let here = path
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| root.clone());
    // Never browse above the root: the page has no business listing the system folders.
    // `starts_with` compares words lexically, so a `..` would slip past it; refuse those.
    if !is_inside_root(&here, &root) {
        return Err(ApiError::new(
            ErrorKind::BadRequest,
            "that folder is outside the storage area",
        ));
    }
    let entries = std::fs::read_dir(&here).map_err(|e| {
        ApiError::new(
            ErrorKind::Failed,
            format!("cannot open {}: {e}. On Android, check that DupeLens is allowed to access all files.", here.display()),
        )
    })?;
    let mut folders: Vec<(String, String)> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            // `file_type` does not follow links, so a link to a folder is skipped on purpose:
            // following it could list the same photos under two names.
            (entry.file_type().ok()?.is_dir() && !name.starts_with('.'))
                .then(|| (name, entry.path().to_string_lossy().into_owned()))
        })
        .collect();
    folders.sort_by_key(|(name, _)| name.to_lowercase());
    let parent = (here != root)
        .then(|| here.parent().map(|p| p.to_string_lossy().into_owned()))
        .flatten();
    Ok(json!({
        "path": here.to_string_lossy(),
        "parent": parent,
        "folders": folders.into_iter().map(|(name, path)| json!({ "name": name, "path": path })).collect::<Vec<_>>(),
    }))
}

/// Well-known folders that exist, offered as one-click choices on first run. Only these
/// locations are listed; nothing is read from them.
fn suggested_folders(app: &tauri::AppHandle) -> Vec<Value> {
    #[cfg(target_os = "android")]
    {
        let _ = app;
        // The folders where a phone keeps photos, videos and downloads.
        let root = browse_root().unwrap_or_else(|| PathBuf::from("/storage/emulated/0"));
        [
            ("Camera", "DCIM/Camera"),
            ("Pictures", "Pictures"),
            ("Screenshots", "Pictures/Screenshots"),
            ("Downloads", "Download"),
            ("Videos", "Movies"),
            ("WhatsApp", "Android/media/com.whatsapp/WhatsApp/Media"),
            ("Documents", "Documents"),
            ("Music", "Music"),
        ]
        .into_iter()
        .filter_map(|(name, rel)| {
            let dir = root.join(rel);
            dir.is_dir()
                .then(|| json!({ "name": name, "path": dir.to_string_lossy() }))
        })
        .collect()
    }
    #[cfg(not(target_os = "android"))]
    {
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
        candidates
            .into_iter()
            .filter_map(|(name, dir)| {
                let dir = dir.ok().filter(|d| d.is_dir())?;
                Some(json!({ "name": name, "path": dir.to_string_lossy() }))
            })
            .collect()
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

// Android has no native folder picker that hands back a plain path (its picker returns
// content:// addresses), so the page draws its own browser (`browse_folder`) and never calls
// this there; picking nothing is the honest answer if it does.
#[cfg(mobile)]
fn pick_folder(_app: &tauri::AppHandle) -> Result<Value, ApiError> {
    Ok(Value::Null)
}

/// Everything both hosts share: plugins, the `api` command and the `thumb` image scheme.
fn builder() -> tauri::Builder<tauri::Wry> {
    let builder = tauri::Builder::default().plugin(tauri_plugin_dupelens_device::init());
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
        .register_asynchronous_uri_scheme_protocol("video", |ctx, request, responder| {
            let service = ctx.app_handle().state::<Service>().inner().clone();
            std::thread::spawn(move || {
                let id = request
                    .uri()
                    .path()
                    .trim_start_matches('/')
                    .parse::<i64>()
                    .ok();
                let range = request
                    .headers()
                    .get("range")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                let response = id
                    .and_then(|id| service.video_path(id).ok())
                    .and_then(|path| video_response(&path, range.as_deref()))
                    .unwrap_or_else(|| {
                        let mut r = http::Response::new(Vec::new());
                        *r.status_mut() = http::StatusCode::NOT_FOUND;
                        r
                    });
                responder.respond(response);
            });
        })
}

/// Largest slice of a video sent per request. The player asks for more as it plays, and asks
/// again at the new spot when the user seeks, so a big file is never read into memory whole.
const VIDEO_CHUNK: u64 = 4 * 1024 * 1024;

fn video_mime(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("mkv") => "video/x-matroska",
        Some("avi") => "video/x-msvideo",
        _ => "video/mp4",
    }
}

/// Reads `bytes=start-end` (either end optional) into an inclusive byte range within `len`.
fn parse_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    let last = len.checked_sub(1)?;
    let (start, end) = if a.is_empty() {
        // "-N": the last N bytes.
        let n: u64 = b.parse().ok()?;
        (len.saturating_sub(n), last)
    } else {
        let start: u64 = a.parse().ok()?;
        let end = if b.is_empty() {
            last
        } else {
            b.parse::<u64>().ok()?.min(last)
        };
        (start, end)
    };
    (start <= end).then_some((start, end))
}

/// A (partial) response for a video file. Without a Range header the player gets the first
/// chunk as a 206 telling it the full size, which is enough for it to start and then seek.
fn video_response(path: &std::path::Path, range: Option<&str>) -> Option<http::Response<Vec<u8>>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len == 0 {
        return None;
    }
    let (start, end) = range
        .and_then(|r| parse_range(r, len))
        .unwrap_or((0, len - 1));
    if start >= len {
        return http::Response::builder()
            .status(416)
            .header("Content-Range", format!("bytes */{len}"))
            .body(Vec::new())
            .ok();
    }
    let end = end.min(start + VIDEO_CHUNK - 1);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut body = vec![0u8; (end - start + 1) as usize];
    file.read_exact(&mut body).ok()?;
    http::Response::builder()
        .status(206)
        .header("Content-Type", video_mime(path))
        .header("Accept-Ranges", "bytes")
        .header("Content-Range", format!("bytes {start}-{end}/{len}"))
        .body(body)
        .ok()
}

#[cfg(test)]
mod video_tests {
    use super::*;

    #[test]
    fn ranges_parse() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("bytes=500-", 1000), Some((500, 999)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=900-5000", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=9-3", 1000), None);
        assert_eq!(parse_range("items=0-1", 1000), None);
    }

    #[test]
    fn video_response_serves_a_slice_with_its_position() {
        let dir = std::env::temp_dir().join(format!("dupelens-video-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clip.mp4");
        std::fs::write(&path, (0..=255u8).cycle().take(1000).collect::<Vec<_>>()).unwrap();
        let r = video_response(&path, Some("bytes=10-19")).unwrap();
        assert_eq!(r.status(), 206);
        assert_eq!(r.headers()["Content-Range"], "bytes 10-19/1000");
        assert_eq!(r.body(), &(10..20u8).collect::<Vec<_>>());
        assert_eq!(video_response(&path, None).unwrap().body().len(), 1000);
        std::fs::remove_dir_all(&dir).ok();
    }
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

#[cfg(test)]
mod tests {
    use super::is_inside_root;
    use std::path::Path;

    #[test]
    fn browse_stays_inside_the_root() {
        let root = Path::new("/storage/emulated/0");
        assert!(is_inside_root(root, root));
        assert!(is_inside_root(
            Path::new("/storage/emulated/0/DCIM/Camera"),
            root
        ));
        assert!(!is_inside_root(Path::new("/storage/emulated"), root));
        assert!(!is_inside_root(
            Path::new("/storage/emulated/0/../../.."),
            root
        ));
        assert!(!is_inside_root(
            Path::new("/storage/emulated/0/DCIM/../.."),
            root
        ));
    }
}
