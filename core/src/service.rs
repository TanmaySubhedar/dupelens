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

use crate::category::Category;
use crate::grouping::GroupingConfig;
use crate::matching::{MatchingSettings, Preset};
use crate::{
    metadata, DeleteMode, DuplicateGroup, Engine, GroupKind, ScanOptions, ScanPhase, ScanProgress,
    ScanSummary,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// The user cancelled a scan or comparison.
    Cancelled,
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
        let kind = match e {
            crate::Error::Cancelled => ErrorKind::Cancelled,
            _ => ErrorKind::Failed,
        };
        Self::new(kind, e.to_string())
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
        /// Bytes read and bytes to read while analysing; zero in other phases.
        bytes_done: u64,
        bytes_total: u64,
    },
    Done {
        summary: UiScanSummary,
    },
    Failed {
        error: String,
    },
    /// The user cancelled. Nothing was lost; a later scan carries on.
    Cancelled,
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
    /// Files that are not photos and had no possible exact twin, so were never read in full.
    pub files_without_twin: usize,
    /// Total size of the files read in full.
    pub bytes_read: u64,
    /// Cloud-only files left alone so they are not downloaded.
    pub cloud_only_skipped: usize,
    /// Extra names for the same file (hard links), left out because deleting one frees nothing.
    pub hard_links_skipped: usize,
    pub files_removed: usize,
    pub unreadable_count: usize,
    pub missing_folders: Vec<String>,
    pub groups: usize,
    pub reclaimable_bytes: u64,
}

/// A file as the UI sees it. Paths are strings (lossy for the rare non-UTF-8 name) so the
/// JSON never fails to serialise.
#[derive(Debug, Clone, Serialize)]
pub struct UiFile {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub folder: String,
    pub category: Category,
    pub size: u64,
    /// Last-modified time, in whole seconds since the Unix epoch.
    pub modified_secs: i64,
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
    /// The kind of file the group holds (that of its first member).
    pub category: Category,
    /// Found only because matching is set looser than Balanced. Review before deleting; the
    /// UI does not pre-select these.
    pub tentative: bool,
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
                    category: Category::from_path(p),
                    size: m.file.size,
                    modified_secs: m.file.mtime_ns / 1_000_000_000,
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
            category: g
                .members
                .first()
                .map_or(Category::Other, |m| Category::from_path(&m.file.path)),
            tentative: g.tentative,
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
    /// Set by `cancel`; read by the running scan or comparison.
    cancel: AtomicBool,
    /// The summary of the last scan, reused (with new group counts) when matching is re-run.
    last_summary: Mutex<Option<UiScanSummary>>,
    results: Mutex<Option<Results>>,
    thumbs: Mutex<ThumbCache>,
}

/// Cheap to clone; every clone shares the same engine and state. `Send + Sync`, so it can
/// live in a Tauri `State` and be used from its command threads.
#[derive(Clone)]
pub struct Service {
    inner: Arc<Inner>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Job {
    Scan,
    Regroup,
}

#[derive(Deserialize)]
struct PathArg {
    path: String,
}

/// A preset as the UI lists it.
#[derive(Debug, Clone, Serialize)]
pub struct UiPreset {
    pub key: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub config: GroupingConfig,
}

/// Everything the settings screen shows.
#[derive(Debug, Clone, Serialize)]
pub struct UiSettings {
    pub preset: Preset,
    pub config: GroupingConfig,
    /// Files smaller than this many bytes are ignored by a scan.
    pub min_size: u64,
    pub presets: Vec<UiPreset>,
}

/// The four raw matching values, for the Custom preset.
#[derive(Deserialize)]
pub struct CustomValues {
    phash_max: u32,
    dhash_max: u32,
    aspect_tolerance: f64,
    max_local_difference: f64,
}

#[derive(Deserialize)]
pub struct SetSettings {
    preset: Option<Preset>,
    /// Required with `preset: "custom"` to change the values; ignored for the other presets.
    custom: Option<CustomValues>,
    min_size: Option<u64>,
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
                cancel: AtomicBool::new(false),
                last_summary: Mutex::new(None),
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
            "start_regroup" => to_value(self.start_regroup()?),
            "cancel" => to_value(self.cancel()),
            "get_settings" => to_value(self.settings()?),
            "set_settings" => to_value(self.set_settings(args(a)?)?),
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
        self.start_job(Job::Scan)
    }

    /// Re-groups the results of the last scan with the current matching setting, on a
    /// background thread, without reading the folders again. Poll [`Service::scan_status`].
    pub fn start_regroup(&self) -> Result<(), ApiError> {
        self.start_job(Job::Regroup)
    }

    /// Asks the running scan or comparison to stop. Returns whether one was running.
    pub fn cancel(&self) -> bool {
        let running = self.is_scanning();
        if running {
            self.inner.cancel.store(true, Ordering::Relaxed);
        }
        running
    }

    fn start_job(&self, job: Job) -> Result<(), ApiError> {
        {
            let mut status = lock(&self.inner.status);
            if matches!(*status, ScanStatus::Running { .. }) {
                return Err(ApiError::busy());
            }
            let engine = self.inner.engine.try_lock().map_err(|_| ApiError::busy())?;
            if job == Job::Scan && engine.folders()?.is_empty() {
                return Err(crate::Error::NoFolders.into());
            }
            self.inner.cancel.store(false, Ordering::Relaxed);
            *status = ScanStatus::Running {
                phase: if job == Job::Scan {
                    JobPhase::Discovering
                } else {
                    JobPhase::Comparing
                },
                done: 0,
                total: 0,
                bytes_done: 0,
                bytes_total: 0,
            };
        }
        let me = self.clone();
        std::thread::spawn(move || {
            let outcome = match job {
                Job::Scan => me.run_scan(),
                Job::Regroup => me.run_regroup(),
            };
            if outcome.is_err() {
                // A job that stops partway may already have changed the database, so the
                // cached groups and thumbnails no longer match it. Drop them; `groups()`
                // rebuilds them from the database on next use.
                me.clear_results();
            }
            *lock(&me.inner.status) = match outcome {
                Ok(summary) => ScanStatus::Done { summary },
                Err(e) if e.kind == ErrorKind::Cancelled => ScanStatus::Cancelled,
                Err(e) => ScanStatus::Failed { error: e.message },
            };
        });
        Ok(())
    }

    fn set_progress(&self, phase: JobPhase, done: usize, total: usize, bytes: (u64, u64)) {
        *lock(&self.inner.status) = ScanStatus::Running {
            phase,
            done,
            total,
            bytes_done: bytes.0,
            bytes_total: bytes.1,
        };
    }

    fn summarize(s: Option<&ScanSummary>, groups: &[DuplicateGroup]) -> UiScanSummary {
        let mut summary = UiScanSummary {
            files_found: 0,
            files_analyzed: 0,
            files_unchanged: 0,
            files_without_twin: 0,
            bytes_read: 0,
            cloud_only_skipped: 0,
            hard_links_skipped: 0,
            files_removed: 0,
            unreadable_count: 0,
            missing_folders: Vec::new(),
            groups: groups.len(),
            reclaimable_bytes: groups.iter().map(|g| g.reclaimable_bytes).sum(),
        };
        if let Some(s) = s {
            summary.files_found = s.files_found;
            summary.files_analyzed = s.files_analyzed;
            summary.files_unchanged = s.files_unchanged;
            summary.files_without_twin = s.files_without_twin;
            summary.bytes_read = s.bytes_read;
            summary.cloud_only_skipped = s.cloud_only_skipped;
            summary.hard_links_skipped = s.hard_links_skipped;
            summary.files_removed = s.files_removed;
            summary.unreadable_count = s.unreadable.len();
            summary.missing_folders = s
                .missing_folders
                .iter()
                .map(|f| f.to_string_lossy().into_owned())
                .collect();
        }
        summary
    }

    fn run_scan(&self) -> Result<UiScanSummary, ApiError> {
        let mut engine = lock(&self.inner.engine);
        let s: ScanSummary = engine.scan_cancellable(
            &|p: ScanProgress| {
                self.set_progress(
                    p.phase.into(),
                    p.done,
                    p.total,
                    (p.bytes_done, p.bytes_total),
                )
            },
            &self.inner.cancel,
        )?;
        self.set_progress(JobPhase::Comparing, 0, 0, (0, 0));
        let groups = engine.groups_cancellable(
            &|d, t| self.set_progress(JobPhase::Comparing, d, t, (0, 0)),
            &self.inner.cancel,
        )?;
        let summary = Self::summarize(Some(&s), &groups);
        *lock(&self.inner.last_summary) = Some(summary.clone());
        self.store_results(&groups);
        Ok(summary)
    }

    fn run_regroup(&self) -> Result<UiScanSummary, ApiError> {
        let engine = lock(&self.inner.engine);
        let groups = engine.groups_cancellable(
            &|d, t| self.set_progress(JobPhase::Comparing, d, t, (0, 0)),
            &self.inner.cancel,
        )?;
        let mut summary = Self::summarize(None, &groups);
        if let Some(last) = &*lock(&self.inner.last_summary) {
            // Keep the scan facts; only the group counts change.
            summary = UiScanSummary {
                groups: summary.groups,
                reclaimable_bytes: summary.reclaimable_bytes,
                ..last.clone()
            };
        }
        *lock(&self.inner.last_summary) = Some(summary.clone());
        self.store_results(&groups);
        Ok(summary)
    }

    /// The matching and scan settings, and the presets to choose from.
    pub fn settings(&self) -> Result<UiSettings, ApiError> {
        let engine = self.engine()?;
        let m = engine.matching();
        Ok(UiSettings {
            preset: m.preset,
            config: m.config,
            min_size: engine.scan_options.min_size,
            presets: Preset::ALL
                .into_iter()
                .map(|p| UiPreset {
                    key: p.key(),
                    name: p.name(),
                    description: p.description(),
                    config: if p == Preset::Custom {
                        m.config
                    } else {
                        p.config()
                    },
                })
                .collect(),
        })
    }

    /// Changes the matching preset (or its raw values) and/or the minimum file size. The
    /// cached results are dropped; call `start_regroup` to rebuild them without a rescan. A
    /// changed minimum size needs a new scan to take effect.
    pub fn set_settings(&self, a: SetSettings) -> Result<UiSettings, ApiError> {
        {
            let mut engine = self.engine()?;
            if let Some(preset) = a.preset {
                let settings = match (preset, a.custom) {
                    (Preset::Custom, Some(c)) => MatchingSettings::custom(GroupingConfig {
                        phash_max: c.phash_max,
                        dhash_max: c.dhash_max,
                        aspect_tolerance: c.aspect_tolerance,
                        max_local_difference: c.max_local_difference,
                        look_alikes: true,
                    })?,
                    (Preset::Custom, None) => MatchingSettings {
                        preset: Preset::Custom,
                        config: engine.matching().config,
                    },
                    (p, _) => MatchingSettings::preset(p),
                };
                engine.set_matching(settings)?;
            }
            if let Some(min_size) = a.min_size {
                engine.set_scan_options(ScanOptions { min_size })?;
            }
        }
        self.clear_results();
        self.settings()
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

    fn scan_to_done(s: &Service) -> UiScanSummary {
        s.call("start_scan", Value::Null).unwrap();
        let ScanStatus::Done { summary } = wait_for_scan(s) else {
            panic!("scan did not finish");
        };
        summary
    }

    /// Two photos that are byte-identical, and a third that is a resized copy.
    fn photo_folder() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let img = RgbImage::from_fn(240, 180, |x, y| {
            let disc = ((x as f64 - 100.0).powi(2) + (y as f64 - 90.0).powi(2)).sqrt() < 50.0;
            if disc {
                Rgb([230, 190, 40])
            } else {
                Rgb([(x / 2) as u8, (y / 2) as u8 + 40, 150])
            }
        });
        img.save(dir.path().join("a.png")).unwrap();
        std::fs::copy(dir.path().join("a.png"), dir.path().join("b.png")).unwrap();
        image::imageops::resize(&img, 120, 90, image::imageops::FilterType::Triangle)
            .save_with_format(dir.path().join("small.jpg"), image::ImageFormat::Jpeg)
            .unwrap();
        dir
    }

    #[test]
    fn settings_and_regroup_through_commands() {
        let dir = photo_folder();
        let s = Service::new(Engine::open_in_memory().unwrap());
        s.call(
            "add_folder",
            json!({ "path": dir.path().to_str().unwrap() }),
        )
        .unwrap();
        scan_to_done(&s);

        let settings = s.call("get_settings", Value::Null).unwrap();
        assert_eq!(settings["preset"], "balanced");
        let keys: Vec<_> = settings["presets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["key"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            keys,
            ["exact_only", "strict", "balanced", "loose", "custom"]
        );
        let members = |s: &Service| {
            s.call("groups", Value::Null).unwrap()[0]["members"]
                .as_array()
                .unwrap()
                .len()
        };
        assert_eq!(members(&s), 3);

        // Exact only: regroup leaves the resized copy out, without reading any folder.
        let changed = s
            .call("set_settings", json!({ "preset": "exact_only" }))
            .unwrap();
        assert_eq!(changed["preset"], "exact_only");
        s.call("start_regroup", Value::Null).unwrap();
        let ScanStatus::Done { summary } = wait_for_scan(&s) else {
            panic!("regroup failed");
        };
        assert_eq!(summary.groups, 1);
        assert_eq!(summary.files_found, 3, "the scan facts are kept");
        assert_eq!(members(&s), 2);

        // Custom values are checked.
        let err = s
            .call(
                "set_settings",
                json!({ "preset": "custom", "custom": {
                    "phash_max": 99, "dhash_max": 12, "aspect_tolerance": 0.05, "max_local_difference": 18.0 }}),
            )
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::Failed);
        let ok = s
            .call(
                "set_settings",
                json!({ "preset": "custom", "min_size": 2048, "custom": {
                    "phash_max": 6, "dhash_max": 10, "aspect_tolerance": 0.05, "max_local_difference": 15.0 }}),
            )
            .unwrap();
        assert_eq!(
            (ok["preset"].as_str(), ok["min_size"].as_u64()),
            (Some("custom"), Some(2048))
        );
        assert_eq!(ok["config"]["phash_max"], 6);
    }

    #[test]
    fn a_job_can_be_cancelled_and_the_service_stays_usable() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..4u8 {
            let data: Vec<u8> = (0..4_000_000usize)
                .map(|n| (n as u8).wrapping_add(i))
                .collect();
            std::fs::create_dir_all(dir.path().join("x")).unwrap();
            std::fs::create_dir_all(dir.path().join("y")).unwrap();
            std::fs::write(dir.path().join(format!("x/{i}.bin")), &data).unwrap();
            std::fs::write(dir.path().join(format!("y/{i}.bin")), &data).unwrap();
        }
        let s = Service::new(Engine::open_in_memory().unwrap());
        s.call(
            "add_folder",
            json!({ "path": dir.path().to_str().unwrap() }),
        )
        .unwrap();
        assert_eq!(
            s.call("cancel", Value::Null).unwrap(),
            json!(false),
            "nothing to cancel"
        );

        s.call("start_scan", Value::Null).unwrap();
        s.cancel();
        let end = wait_for_scan(&s);
        // The scan may finish before the cancel is seen; either way the service stays usable.
        assert!(
            matches!(end, ScanStatus::Cancelled | ScanStatus::Done { .. }),
            "{end:?}"
        );

        let summary = scan_to_done(&s);
        assert_eq!(summary.groups, 4);
        assert!(
            s.call("groups", Value::Null)
                .unwrap()
                .as_array()
                .unwrap()
                .len()
                == 4
        );
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
