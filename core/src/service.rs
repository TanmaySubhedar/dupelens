//! The engine as an app backend: one object that every UI host shares.
//!
//! A UI never calls [`Engine`] directly. It sends a command name plus JSON arguments to
//! [`Service::call`] and gets JSON back, and loads photo previews with
//! [`Service::thumbnail`]. The host only moves those messages. The one host is the Tauri app
//! (`desktop/`): it registers a command, `api(cmd, args)`, that forwards to [`Service::call`],
//! plus a `thumb://` URI scheme that forwards to [`Service::thumbnail`].
//!
//! Commands that need the host's own windowing (a native folder dialog, quitting) are not
//! here; the host implements `pick_folder` and `quit` itself. The full contract is in
//! `docs/UI.md`.
//!
//! Scans run on a background thread so the UI can poll `scan_status` while they progress.
//! Results are cached so that thumbnails and repeated `groups` calls never wait on the
//! database.

use crate::{metadata, DeleteMode, DuplicateGroup, Engine, GroupKind, ScanPhase, ScanSummary};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

/// Longest side of the previews shown in the photo grid and in the enlarged view.
const THUMB_SMALL: u32 = 360;
const THUMB_LARGE: u32 = 1600;
/// Rough cap on cached preview bytes; the cache is emptied when it grows past this.
const THUMB_CACHE_BYTES: usize = 96 * 1024 * 1024;

/// An error a UI can show. `kind` lets hosts pick a status code; `message` is plain language.
#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// Unknown command or malformed arguments: a bug in the UI.
    BadRequest,
    NotFound,
    /// A scan is running; try again when it finishes.
    Busy,
    /// The engine refused or failed (invalid folder, deletion refused, I/O error...).
    Failed,
}

impl ApiError {
    fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
    fn busy() -> Self {
        Self::new(
            ErrorKind::Busy,
            "DupeLens is busy scanning or comparing photos; try again in a moment",
        )
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

impl From<crate::Error> for ApiError {
    fn from(e: crate::Error) -> Self {
        Self::new(ErrorKind::Failed, e.to_string())
    }
}

/// What the scan job is doing, as reported by `scan_status`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ScanStatus {
    Idle,
    Running {
        phase: JobPhase,
        done: usize,
        total: usize,
    },
    Done {
        summary: UiScanSummary,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobPhase {
    Discovering,
    Analyzing,
    Saving,
    /// Building groups, including the pixel check of look-alike candidates.
    Comparing,
}

impl From<ScanPhase> for JobPhase {
    fn from(p: ScanPhase) -> Self {
        match p {
            ScanPhase::Discovering => JobPhase::Discovering,
            ScanPhase::Analyzing => JobPhase::Analyzing,
            ScanPhase::Saving => JobPhase::Saving,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UiScanSummary {
    pub files_found: usize,
    pub files_analyzed: usize,
    pub files_unchanged: usize,
    pub files_removed: usize,
    pub unreadable_count: usize,
    pub missing_folders: Vec<String>,
    pub groups: usize,
    pub reclaimable_bytes: u64,
}

/// A photo as the UI sees it. Paths are strings (lossy for the rare non-UTF-8 name) so the
/// JSON never fails to serialise.
#[derive(Debug, Clone, Serialize)]
pub struct UiFile {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub folder: String,
    pub size: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub taken_at: Option<String>,
    pub favorite: bool,
    pub identical_to_recommended: bool,
    pub distance_to_recommended: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiGroup {
    pub id: i64,
    pub kind: GroupKind,
    pub recommended_id: i64,
    pub reason: String,
    pub reclaimable_bytes: u64,
    pub members: Vec<UiFile>,
}

impl From<&DuplicateGroup> for UiGroup {
    fn from(g: &DuplicateGroup) -> Self {
        let members = g
            .members
            .iter()
            .map(|m| {
                let p = &m.file.path;
                UiFile {
                    id: m.file.id,
                    path: p.to_string_lossy().into_owned(),
                    name: p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    folder: p
                        .parent()
                        .map(|d| d.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    size: m.file.size,
                    width: m.file.width,
                    height: m.file.height,
                    taken_at: m.file.taken_at.clone(),
                    favorite: m.file.favorite,
                    identical_to_recommended: m.identical_to_recommended,
                    distance_to_recommended: m.distance_to_recommended,
                }
            })
            .collect();
        Self {
            id: g.id,
            kind: g.kind,
            recommended_id: g.recommended_id,
            reason: g.reason.clone(),
            reclaimable_bytes: g.reclaimable_bytes,
            members,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UiDeleteReport {
    pub deleted: Vec<i64>,
    pub failed: Vec<UiDeleteFailure>,
    pub bytes_freed: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiDeleteFailure {
    pub id: i64,
    pub path: String,
    pub reason: String,
}

/// Groups from the last scan plus a lookup of which file ids they contain. Only files in
/// here can be previewed, so a UI can never read an arbitrary path through `thumbnail`.
struct Results {
    groups: Arc<Vec<UiGroup>>,
    paths: HashMap<i64, PathBuf>,
}

impl Results {
    fn new(groups: &[DuplicateGroup]) -> Self {
        let paths = groups
            .iter()
            .flat_map(|g| g.members.iter())
            .map(|m| (m.file.id, m.file.path.clone()))
            .collect();
        Self {
            groups: Arc::new(groups.iter().map(UiGroup::from).collect()),
            paths,
        }
    }
}

#[derive(Default)]
struct ThumbCache {
    entries: HashMap<(i64, u32), Arc<Vec<u8>>>,
    bytes: usize,
}

struct Inner {
    engine: Mutex<Engine>,
    status: Mutex<ScanStatus>,
    results: Mutex<Option<Results>>,
    thumbs: Mutex<ThumbCache>,
}

/// Cheap to clone; every clone shares the same engine and state. `Send + Sync`, so it can
/// live in a Tauri `State` and be used from its command threads.
#[derive(Clone)]
pub struct Service {
    inner: Arc<Inner>,
}

#[derive(Deserialize)]
struct PathArg {
    path: String,
}

#[derive(Deserialize)]
struct DeleteArgs {
    ids: Vec<i64>,
    #[serde(default)]
    permanent: bool,
}

fn args<T: for<'de> Deserialize<'de>>(v: Value) -> Result<T, ApiError> {
    serde_json::from_value(v).map_err(|e| ApiError::new(ErrorKind::BadRequest, e.to_string()))
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic elsewhere must not lock the user out of the app; the data is still usable.
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn to_value<T: Serialize>(v: T) -> Result<Value, ApiError> {
    serde_json::to_value(v).map_err(|e| ApiError::new(ErrorKind::Failed, e.to_string()))
}

impl Service {
    pub fn new(engine: Engine) -> Self {
        Self {
            inner: Arc::new(Inner {
                engine: Mutex::new(engine),
                status: Mutex::new(ScanStatus::Idle),
                results: Mutex::new(None),
                thumbs: Mutex::new(ThumbCache::default()),
            }),
        }
    }

    /// Runs one UI command. See `docs/UI.md` for the list, arguments and results.
    pub fn call(&self, cmd: &str, a: Value) -> Result<Value, ApiError> {
        match cmd {
            "info" => Ok(json!({
                "version": env!("CARGO_PKG_VERSION"),
                "trash_supported": crate::trash_supported(),
                "platform": std::env::consts::OS,
            })),
            "folders" => to_value(self.folders()?),
            "add_folder" => to_value(self.add_folder(Path::new(&args::<PathArg>(a)?.path))?),
            "remove_folder" => to_value(self.remove_folder(Path::new(&args::<PathArg>(a)?.path))?),
            "start_scan" => to_value(self.start_scan()?),
            "scan_status" => to_value(self.scan_status()),
            "groups" => to_value(&*self.groups()?),
            "delete" => {
                let d: DeleteArgs = args(a)?;
                to_value(self.delete(&d.ids, d.permanent)?)
            }
            _ => Err(ApiError::new(
                ErrorKind::BadRequest,
                format!("unknown command `{cmd}`"),
            )),
        }
    }

    fn is_scanning(&self) -> bool {
        matches!(*lock(&self.inner.status), ScanStatus::Running { .. })
    }

    /// The engine, unless a scan holds it.
    fn engine(&self) -> Result<MutexGuard<'_, Engine>, ApiError> {
        if self.is_scanning() {
            return Err(ApiError::busy());
        }
        self.inner.engine.try_lock().map_err(|_| ApiError::busy())
    }

    pub fn folders(&self) -> Result<Vec<String>, ApiError> {
        Ok(self
            .engine()?
            .folders()?
            .iter()
            .map(|f| f.to_string_lossy().into_owned())
            .collect())
    }

    pub fn add_folder(&self, path: &Path) -> Result<String, ApiError> {
        let path = path.to_str().map(str::trim).map(PathBuf::from);
        let path = path.filter(|p| !p.as_os_str().is_empty()).ok_or_else(|| {
            ApiError::new(ErrorKind::Failed, "enter the path of a folder to scan")
        })?;
        Ok(self
            .engine()?
            .add_folder(&path)?
            .to_string_lossy()
            .into_owned())
    }

    pub fn remove_folder(&self, path: &Path) -> Result<bool, ApiError> {
        Ok(self.engine()?.remove_folder(path)?)
    }

    /// Starts a scan on a background thread. Poll [`Service::scan_status`] for progress.
    pub fn start_scan(&self) -> Result<(), ApiError> {
        {
            let mut status = lock(&self.inner.status);
            if matches!(*status, ScanStatus::Running { .. }) {
                return Err(ApiError::busy());
            }
            let engine = self.inner.engine.try_lock().map_err(|_| ApiError::busy())?;
            if engine.folders()?.is_empty() {
                return Err(crate::Error::NoFolders.into());
            }
            *status = ScanStatus::Running {
                phase: JobPhase::Discovering,
                done: 0,
                total: 0,
            };
        }
        let me = self.clone();
        std::thread::spawn(move || {
            let outcome = me.run_scan();
            if outcome.is_err() {
                // A scan that fails partway may already have changed the database, so the
                // cached groups and thumbnails no longer match it. Drop them; `groups()`
                // rebuilds them from the database on next use.
                me.clear_results();
            }
            *lock(&me.inner.status) = match outcome {
                Ok(summary) => ScanStatus::Done { summary },
                Err(e) => ScanStatus::Failed { error: e.message },
            };
        });
        Ok(())
    }

    fn set_progress(&self, phase: JobPhase, done: usize, total: usize) {
        *lock(&self.inner.status) = ScanStatus::Running { phase, done, total };
    }

    fn run_scan(&self) -> Result<UiScanSummary, ApiError> {
        let mut engine = lock(&self.inner.engine);
        let s: ScanSummary =
            engine.scan(&|p| self.set_progress(p.phase.into(), p.done, p.total))?;
        self.set_progress(JobPhase::Comparing, 0, 0);
        let groups = engine.groups()?;
        let summary = UiScanSummary {
            files_found: s.files_found,
            files_analyzed: s.files_analyzed,
            files_unchanged: s.files_unchanged,
            files_removed: s.files_removed,
            unreadable_count: s.unreadable.len(),
            missing_folders: s
                .missing_folders
                .iter()
                .map(|f| f.to_string_lossy().into_owned())
                .collect(),
            groups: groups.len(),
            reclaimable_bytes: groups.iter().map(|g| g.reclaimable_bytes).sum(),
        };
        self.store_results(&groups);
        Ok(summary)
    }

    fn clear_results(&self) {
        *lock(&self.inner.results) = None;
        *lock(&self.inner.thumbs) = ThumbCache::default();
    }

    fn store_results(&self, groups: &[DuplicateGroup]) {
        *lock(&self.inner.results) = Some(Results::new(groups));
        *lock(&self.inner.thumbs) = ThumbCache::default();
    }

    pub fn scan_status(&self) -> ScanStatus {
        lock(&self.inner.status).clone()
    }

    /// Duplicate groups from the most recent scan (computed once, then cached).
    pub fn groups(&self) -> Result<Arc<Vec<UiGroup>>, ApiError> {
        if let Some(r) = &*lock(&self.inner.results) {
            return Ok(r.groups.clone());
        }
        let groups = self.engine()?.groups()?;
        self.store_results(&groups);
        Ok(lock(&self.inner.results)
            .as_ref()
            .map(|r| r.groups.clone())
            .unwrap_or_default())
    }

    /// Deletes exactly `ids` (the engine's safety rules apply), then refreshes the groups.
    pub fn delete(&self, ids: &[i64], permanent: bool) -> Result<UiDeleteReport, ApiError> {
        let mode = if permanent {
            DeleteMode::Permanent
        } else {
            DeleteMode::Trash
        };
        let mut engine = self.engine()?;
        let paths: HashMap<i64, PathBuf> = lock(&self.inner.results)
            .as_ref()
            .map(|r| r.paths.clone())
            .unwrap_or_default();
        let report = engine.delete(ids, mode)?;
        let groups = engine.groups()?;
        drop(engine);
        self.store_results(&groups);
        Ok(UiDeleteReport {
            deleted: report.deleted,
            failed: report
                .failed
                .into_iter()
                .map(|(id, reason)| UiDeleteFailure {
                    id,
                    path: paths
                        .get(&id)
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    reason,
                })
                .collect(),
            bytes_freed: report.bytes_freed,
        })
    }

    /// A JPEG preview of a file in the current results. `large` picks the enlarged-view size.
    ///
    /// Errors with `NotFound` for ids outside the current results and for files that cannot
    /// be previewed (e.g. HEIC); the UI then shows a placeholder.
    pub fn thumbnail(&self, id: i64, large: bool) -> Result<Arc<Vec<u8>>, ApiError> {
        let side = if large { THUMB_LARGE } else { THUMB_SMALL };
        if let Some(hit) = lock(&self.inner.thumbs).entries.get(&(id, side)) {
            return Ok(hit.clone());
        }
        let path = lock(&self.inner.results)
            .as_ref()
            .and_then(|r| r.paths.get(&id).cloned())
            .ok_or_else(|| ApiError::new(ErrorKind::NotFound, "no such photo in the results"))?;
        // Decoding runs without any lock held, so several previews render in parallel.
        let jpeg = metadata::preview_jpeg(&path, side)
            .ok()
            .flatten()
            .ok_or_else(|| ApiError::new(ErrorKind::NotFound, "no preview for this file"))?;
        let jpeg = Arc::new(jpeg);
        let mut cache = lock(&self.inner.thumbs);
        if cache.bytes + jpeg.len() > THUMB_CACHE_BYTES {
            *cache = ThumbCache::default();
        }
        cache.bytes += jpeg.len();
        cache.entries.insert((id, side), jpeg.clone());
        Ok(jpeg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use std::time::{Duration, Instant};

    fn wait_for_scan(s: &Service) -> ScanStatus {
        let start = Instant::now();
        loop {
            let st = s.scan_status();
            if !matches!(st, ScanStatus::Running { .. }) {
                return st;
            }
            assert!(
                start.elapsed() < Duration::from_secs(60),
                "scan never finished"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn full_flow_through_commands() {
        let dir = tempfile::tempdir().unwrap();
        let img = RgbImage::from_fn(80, 60, |x, y| Rgb([(x * 3) as u8, (y * 4) as u8, 120]));
        img.save(dir.path().join("a.png")).unwrap();
        std::fs::copy(dir.path().join("a.png"), dir.path().join("b.png")).unwrap();

        let s = Service::new(Engine::open_in_memory().unwrap());
        let err = s.call("start_scan", Value::Null).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Failed, "no folders yet");

        let folder = dir.path().to_str().unwrap();
        s.call("add_folder", json!({ "path": folder })).unwrap();
        assert_eq!(
            s.call("folders", Value::Null)
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );

        s.call("start_scan", Value::Null).unwrap();
        let ScanStatus::Done { summary } = wait_for_scan(&s) else {
            panic!("scan failed");
        };
        assert_eq!((summary.files_found, summary.groups), (2, 1));

        let groups = s.call("groups", Value::Null).unwrap();
        let group = &groups[0];
        assert_eq!(group["kind"], "exact");
        let keep = group["recommended_id"].as_i64().unwrap();
        let other: Vec<i64> = group["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_i64().unwrap())
            .filter(|&id| id != keep)
            .collect();

        let jpeg = s.thumbnail(keep, false).unwrap();
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8], "a JPEG");
        assert_eq!(
            s.thumbnail(9999, false).unwrap_err().kind,
            ErrorKind::NotFound
        );

        // Deleting every copy is refused by the engine and reported, not panicked on.
        let all: Vec<i64> = other.iter().copied().chain([keep]).collect();
        let err = s
            .call("delete", json!({ "ids": all, "permanent": true }))
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::Failed);

        let report = s
            .call("delete", json!({ "ids": other, "permanent": true }))
            .unwrap();
        assert_eq!(report["deleted"].as_array().unwrap().len(), 1);
        assert_eq!(s.call("groups", Value::Null).unwrap(), json!([]));
    }

    #[test]
    fn unknown_command_and_bad_args_are_bad_requests() {
        let s = Service::new(Engine::open_in_memory().unwrap());
        assert_eq!(
            s.call("nope", Value::Null).unwrap_err().kind,
            ErrorKind::BadRequest
        );
        assert_eq!(
            s.call("delete", json!({ "ids": "x" })).unwrap_err().kind,
            ErrorKind::BadRequest
        );
    }
}
