//! DupeLens core engine: offline duplicate photo detection.
//!
//! Nothing here touches the network. Files are only opened for reading until the user
//! explicitly asks [`Engine::delete`] to remove specific files.

pub mod category;
#[cfg(feature = "cli")]
pub mod cli;
pub mod db;
pub mod error;
pub mod grouping;
pub mod hashing;
pub mod metadata;
pub mod model;
pub mod recommend;
pub mod scanner;
pub mod service;

pub use error::{Error, Result};
pub use grouping::GroupingConfig;
pub use model::{DuplicateGroup, FileRecord, GroupKind, GroupMember};
pub use scanner::ScanOptions;

use rayon::prelude::*;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanPhase {
    Discovering,
    Analyzing,
    Saving,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ScanProgress {
    pub phase: ScanPhase,
    /// Files finished and files to do, in this phase.
    pub done: usize,
    pub total: usize,
    /// Bytes read and bytes to read, while analysing. A single huge file moves this bar even
    /// though `done` stays put until it is finished.
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl ScanProgress {
    fn new(phase: ScanPhase, done: usize, total: usize) -> Self {
        Self {
            phase,
            done,
            total,
            bytes_done: 0,
            bytes_total: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanSummary {
    pub files_found: usize,
    /// Files that were new or changed and were read in full: photos (analysed) and other files
    /// that might have an identical twin (hashed).
    pub files_analyzed: usize,
    /// Files that are not photos and could not have an exact twin (no other file has their
    /// size, or none that matches on the first and last 64 KiB), so they were not read in full.
    pub files_without_twin: usize,
    /// Total size of the files read in full during this scan.
    pub bytes_read: u64,
    /// Cloud-only files (e.g. OneDrive "Files On-Demand") left alone so they are not downloaded.
    pub cloud_only_skipped: usize,
    /// Extra names for a file already in the list (hard links). Deleting one would free no
    /// space, so they are left out rather than shown as duplicates.
    pub hard_links_skipped: usize,
    /// Files whose cached analysis was reused because size and mtime were unchanged.
    pub files_unchanged: usize,
    /// Database entries removed because the file no longer exists in a selected folder.
    pub files_removed: usize,
    pub unreadable: Vec<PathBuf>,
    pub missing_folders: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteMode {
    /// Move to the Recycle Bin / Trash so the user can restore it.
    Trash,
    Permanent,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DeleteReport {
    pub deleted: Vec<i64>,
    pub failed: Vec<(i64, String)>,
    pub bytes_freed: u64,
}

/// Version of [`hashing::local_difference`]; bump when its algorithm changes so cached
/// scores are recomputed.
const LOCAL_DIFFERENCE_METHOD: i64 = 5;

/// Whether [`DeleteMode::Trash`] works on this platform.
pub const fn trash_supported() -> bool {
    cfg!(any(
        target_os = "windows",
        target_os = "macos",
        all(unix, not(any(target_os = "android", target_os = "ios")))
    ))
}

/// One file to be read in full during a scan.
struct Job {
    file: scanner::DiscoveredFile,
    /// The database row of a file already known without a hash; `None` for a new or changed file.
    existing_id: Option<i64>,
    /// Photos get the full analysis; everything else is only hashed.
    photo: bool,
}

pub struct Engine {
    conn: Connection,
    pub grouping: GroupingConfig,
    pub scan_options: ScanOptions,
}

impl Engine {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        Ok(Self {
            conn: db::open(db_path)?,
            grouping: GroupingConfig::default(),
            scan_options: ScanOptions::default(),
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        Ok(Self {
            conn: db::open_in_memory()?,
            grouping: GroupingConfig::default(),
            scan_options: ScanOptions::default(),
        })
    }

    /// Remembers a folder for scanning. Returns the canonical path that was stored.
    ///
    /// Canonicalising matters for safety: if one folder were stored under two spellings
    /// (`C:\Photos` and `c:\photos`, or via a junction), every photo would be scanned twice
    /// and look like an exact duplicate of itself, and "deleting the copy" would delete the
    /// only file.
    pub fn add_folder(&self, folder: &Path) -> Result<PathBuf> {
        if !folder.is_dir() {
            return Err(Error::InvalidFolder(folder.to_path_buf()));
        }
        let canonical = canonical_folder(folder).map_err(|e| Error::io(folder, e))?;
        if canonical.to_str().is_none() {
            return Err(Error::InvalidFolder(canonical));
        }
        db::add_folder(&self.conn, &canonical)?;
        Ok(canonical)
    }

    /// Forgets a folder. Its files drop out of results on the next scan.
    pub fn remove_folder(&self, folder: &Path) -> Result<bool> {
        // The folder may no longer exist (e.g. an unplugged drive); fall back to its absolute path.
        let path = canonical_folder(folder)
            .or_else(|_| std::path::absolute(folder))
            .map_err(|e| Error::io(folder, e))?;
        db::remove_folder(&self.conn, &path)
    }

    pub fn folders(&self) -> Result<Vec<PathBuf>> {
        db::folders(&self.conn)
    }

    /// Scans the selected folders (and only those). Unchanged files reuse cached results.
    ///
    /// Photos are always read and analysed in full, for look-alike matching. Every other file
    /// takes part in exact matching only, and is read as little as possible:
    /// 1. a file whose size no other file shares cannot have a twin, so it is never read;
    /// 2. files that share a size are compared by a cheap fingerprint of their first and last
    ///    64 KiB;
    /// 3. only files that still match (or share a size with a file already hashed) are read in
    ///    full and hashed.
    ///
    /// This keeps a library of large videos and music cheap to scan: only real or very likely
    /// duplicates are ever read end to end.
    pub fn scan(&mut self, progress: &(dyn Fn(ScanProgress) + Sync)) -> Result<ScanSummary> {
        self.scan_cancellable(progress, &AtomicBool::new(false))
    }

    /// [`Engine::scan`] that stops once `cancel` is set, returning [`Error::Cancelled`].
    ///
    /// Nothing is half-written: everything is saved in one transaction at the end. If the scan
    /// is cancelled while reading files, the files already read are saved first (so the next
    /// scan does not repeat that work), but nothing is forgotten or removed.
    pub fn scan_cancellable(
        &mut self,
        progress: &(dyn Fn(ScanProgress) + Sync),
        cancel: &AtomicBool,
    ) -> Result<ScanSummary> {
        let folders = self.folders()?;
        if folders.is_empty() {
            return Err(Error::NoFolders);
        }
        let mut summary = ScanSummary::default();
        let (present, missing): (Vec<_>, Vec<_>) = folders.into_iter().partition(|f| f.is_dir());
        summary.missing_folders = missing;

        progress(ScanProgress::new(ScanPhase::Discovering, 0, 0));
        let mut discovery = scanner::discover_cancellable(&present, &self.scan_options, cancel);
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let (files, hard_links) = scanner::remove_hard_links(std::mem::take(&mut discovery.files));
        discovery.files = files;
        summary.hard_links_skipped = hard_links;
        summary.files_found = discovery.files.len();
        summary.cloud_only_skipped = discovery.cloud_only;
        summary.unreadable = discovery.skipped;

        let known = db::fingerprints(&self.conn)?;
        let mut size_count: HashMap<u64, usize> = HashMap::new();
        for f in &discovery.files {
            *size_count.entry(f.size).or_default() += 1;
        }
        let shares_size = |f: &scanner::DiscoveredFile| size_count[&f.size] > 1;

        let (unchanged, changed): (Vec<_>, Vec<_>) = discovery.files.into_iter().partition(|f| {
            known
                .get(&f.path)
                .is_some_and(|k| k.size == f.size && k.mtime_ns == f.mtime_ns)
        });
        summary.files_unchanged = unchanged.len();

        // Sizes of files that are (or are about to be) fully hashed anyway.
        let mut hashed_sizes: HashSet<u64> = HashSet::new();
        let mut jobs: Vec<Job> = Vec::new();
        let mut candidates: Vec<(scanner::DiscoveredFile, Option<i64>)> = Vec::new();
        let mut unhashed_new: Vec<scanner::DiscoveredFile> = Vec::new();
        let mut seen_ids: Vec<i64> = Vec::new();

        for f in unchanged {
            let k = &known[&f.path];
            if k.hashed {
                hashed_sizes.insert(f.size);
                seen_ids.push(k.id);
            } else if shares_size(&f) {
                // Was unique before; another file of its size has appeared since.
                candidates.push((f, Some(k.id)));
            } else {
                summary.files_without_twin += 1;
                seen_ids.push(k.id);
            }
        }
        for f in changed {
            if scanner::is_image(&f.path) {
                hashed_sizes.insert(f.size);
                jobs.push(Job {
                    file: f,
                    existing_id: None,
                    photo: true,
                });
            } else if shares_size(&f) {
                candidates.push((f, None));
            } else {
                summary.files_without_twin += 1;
                unhashed_new.push(f);
            }
        }

        // Step 2: a cheap fingerprint of every candidate that shares its size with another file.
        progress(ScanProgress::new(ScanPhase::Analyzing, 0, 0));
        let fingerprinted: Vec<_> = candidates
            .into_par_iter()
            .map(|(f, id)| {
                let r = hashing::partial_hash(&f.path, f.size);
                (f, id, r)
            })
            .collect();
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let mut same_partial: HashMap<(u64, &str), usize> = HashMap::new();
        for (f, _, r) in &fingerprinted {
            if let Ok(h) = r {
                *same_partial.entry((f.size, h.as_str())).or_default() += 1;
            }
        }
        let needs_full: Vec<bool> = fingerprinted
            .iter()
            .map(|(f, _, r)| match r {
                Ok(h) => hashed_sizes.contains(&f.size) || same_partial[&(f.size, h.as_str())] > 1,
                Err(_) => false,
            })
            .collect();
        for ((f, id, r), full) in fingerprinted.into_iter().zip(needs_full) {
            match (r, full) {
                (Err(_), _) => summary.unreadable.push(f.path),
                (Ok(_), true) => jobs.push(Job {
                    file: f,
                    existing_id: id,
                    photo: false,
                }),
                (Ok(_), false) => {
                    summary.files_without_twin += 1;
                    match id {
                        Some(id) => seen_ids.push(id),
                        None => unhashed_new.push(f),
                    }
                }
            }
        }

        // Step 3: read in full.
        let total = jobs.len();
        let bytes_total: u64 = jobs.iter().map(|j| j.file.size).sum();
        let done = AtomicUsize::new(0);
        let bytes_done = AtomicU64::new(0);
        let report = |files_done: usize, bytes: u64| {
            progress(ScanProgress {
                phase: ScanPhase::Analyzing,
                done: files_done,
                total,
                bytes_done: bytes,
                bytes_total,
            })
        };
        report(0, 0);
        let analysed: Vec<_> = jobs
            .into_par_iter()
            .map(|job| {
                if cancel.load(Ordering::Relaxed) {
                    return (job, Err(std::io::ErrorKind::Interrupted.into()));
                }
                let result = if job.photo {
                    let r = metadata::analyze(&job.file.path, job.file.size);
                    bytes_done.fetch_add(job.file.size, Ordering::Relaxed);
                    r
                } else {
                    metadata::analyze_plain(&job.file.path, &mut |n| {
                        let b = bytes_done.fetch_add(n, Ordering::Relaxed) + n;
                        report(done.load(Ordering::Relaxed), b);
                        !cancel.load(Ordering::Relaxed)
                    })
                };
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                report(n, bytes_done.load(Ordering::Relaxed));
                (job, result)
            })
            .collect();
        let cancelled = cancel.load(Ordering::Relaxed);

        progress(ScanProgress::new(ScanPhase::Saving, 0, 0));
        let tx = self.conn.transaction()?;
        let scan_id = db::next_scan_id(&tx)?;
        for id in seen_ids {
            db::mark_seen(&tx, id, scan_id)?;
        }
        for f in &unhashed_new {
            db::upsert_unhashed(&tx, &f.path, f.size, f.mtime_ns, scan_id)?;
        }
        for (job, result) in analysed {
            match result {
                Ok(a) => {
                    match job.existing_id {
                        Some(id) => db::set_hash(&tx, id, &a.sha256, scan_id)?,
                        None => db::upsert(
                            &tx,
                            &job.file.path,
                            job.file.size,
                            job.file.mtime_ns,
                            &a,
                            scan_id,
                        )?,
                    }
                    summary.files_analyzed += 1;
                    summary.bytes_read += job.file.size;
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => summary.unreadable.push(job.file.path),
            }
        }
        if cancelled {
            // Keep what was read, but forget nothing: files not reached yet look "unseen".
            tx.commit()?;
            return Err(Error::Cancelled);
        }
        summary.files_removed = db::prune_unseen(&tx, scan_id)?;
        db::prune_local_differences(&tx, LOCAL_DIFFERENCE_METHOD)?;
        tx.commit()?;
        Ok(summary)
    }

    /// Duplicate groups from the most recent scan, largest reclaimable space first.
    ///
    /// Look-alike candidates are compared pixel by pixel (reading the two files) the first
    /// time they meet; the score is cached, so later calls only read the database.
    pub fn groups(&self) -> Result<Vec<DuplicateGroup>> {
        self.groups_cancellable(&|_, _| {}, &AtomicBool::new(false))
    }

    /// [`Engine::groups`] with progress (`done`, `total` look-alike pairs compared) and
    /// cancellation, which returns [`Error::Cancelled`]. Scores computed before a cancel are
    /// kept, so running it again carries on where it stopped.
    pub fn groups_cancellable(
        &self,
        progress: &(dyn Fn(usize, usize) + Sync),
        cancel: &AtomicBool,
    ) -> Result<Vec<DuplicateGroup>> {
        let files = db::all_files(&self.conn)?;
        let key = |a: &FileRecord, b: &FileRecord| {
            let (x, y) = db::ordered(&a.sha256, &b.sha256);
            (x.to_owned(), y.to_owned())
        };

        // Pixel-check every look-alike candidate: reuse cached scores, and compute the rest on
        // all CPU cores at once (each check decodes two photos, so this is the slow part).
        let mut scores: HashMap<(String, String), f64> = HashMap::new();
        let mut missing = Vec::new();
        for (i, j) in grouping::near_candidates(&files, &self.grouping) {
            let k = key(&files[i], &files[j]);
            match db::cached_local_difference(&self.conn, &k.0, &k.1, LOCAL_DIFFERENCE_METHOD)? {
                Some(score) => {
                    scores.insert(k, score);
                }
                None => missing.push((i, j, k)),
            }
        }
        let total = missing.len();
        let compared = AtomicUsize::new(0);
        progress(0, total);
        let computed: Vec<_> = missing
            .into_par_iter()
            .map(|(i, j, k)| {
                if cancel.load(Ordering::Relaxed) {
                    return (k, None);
                }
                // Unreadable or undecodable right now (e.g. file moved): no score, no match.
                let score = match (
                    metadata::load_gray(&files[i].path),
                    metadata::load_gray(&files[j].path),
                ) {
                    (Ok(Some(a)), Ok(Some(b))) => Some(hashing::local_difference(&a, &b)),
                    _ => None,
                };
                progress(compared.fetch_add(1, Ordering::Relaxed) + 1, total);
                (k, score)
            })
            .collect();
        for (k, score) in computed {
            if let Some(score) = score {
                db::store_local_difference(&self.conn, &k.0, &k.1, LOCAL_DIFFERENCE_METHOD, score)?;
                scores.insert(k, score);
            }
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }

        Ok(grouping::build_groups(
            files,
            &self.grouping,
            &mut |a, b| scores.get(&key(a, b)).copied(),
        ))
    }

    /// Deletes exactly the given files after the user has confirmed.
    ///
    /// Refuses the whole request (deleting nothing) if a file is unknown or if the request
    /// would remove every copy in a duplicate group. Individual files are skipped (reported in
    /// `failed`) if they changed on disk since the scan or lie outside the selected folders.
    pub fn delete(&mut self, file_ids: &[i64], mode: DeleteMode) -> Result<DeleteReport> {
        if mode == DeleteMode::Trash && !trash_supported() {
            return Err(Error::DeletionRefused(
                "moving to trash is not supported on this platform".into(),
            ));
        }
        let requested: HashSet<i64> = file_ids.iter().copied().collect();

        let mut files = Vec::with_capacity(requested.len());
        for &id in &requested {
            match db::file_by_id(&self.conn, id)? {
                Some(f) => files.push(f),
                None => {
                    return Err(Error::DeletionRefused(format!(
                        "unknown file id {id}; rescan first"
                    )))
                }
            }
        }

        for g in self.groups()? {
            if g.members.iter().all(|m| requested.contains(&m.file.id)) {
                return Err(Error::DeletionRefused(format!(
                    "this would delete every copy in group {} (keep at least one)",
                    g.id
                )));
            }
        }

        let folders = self.folders()?;
        let mut report = DeleteReport::default();
        files.sort_by_key(|f| f.id);
        for f in files {
            match self.delete_one(&f, &folders, mode) {
                Ok(()) => {
                    db::delete_file_row(&self.conn, f.id)?;
                    report.bytes_freed += f.size;
                    report.deleted.push(f.id);
                }
                Err(why) => report.failed.push((f.id, why)),
            }
        }
        Ok(report)
    }

    fn delete_one(
        &self,
        f: &FileRecord,
        folders: &[PathBuf],
        mode: DeleteMode,
    ) -> std::result::Result<(), String> {
        if !folders.iter().any(|root| f.path.starts_with(root)) {
            return Err("outside the selected folders".into());
        }
        let meta =
            std::fs::symlink_metadata(&f.path).map_err(|e| format!("cannot read file: {e}"))?;
        if !meta.is_file() {
            return Err("no longer a regular file".into());
        }
        if meta.len() != f.size || scanner::mtime_ns(&meta) != f.mtime_ns {
            return Err("file changed since the last scan; rescan before deleting".into());
        }
        match mode {
            DeleteMode::Permanent => std::fs::remove_file(&f.path).map_err(|e| e.to_string()),
            DeleteMode::Trash => move_to_trash(&f.path),
        }
    }
}

/// The one true spelling of a folder: real letter case, links and `subst` drives resolved.
/// On Windows, strips the `\\?\` prefix that `canonicalize` adds, so paths stay readable.
fn canonical_folder(folder: &Path) -> std::io::Result<PathBuf> {
    let canonical = std::fs::canonicalize(folder)?;
    #[cfg(windows)]
    {
        if let Some(s) = canonical.to_str() {
            if let Some(unc) = s.strip_prefix(r"\\?\UNC\") {
                return Ok(PathBuf::from(format!(r"\\{unc}")));
            }
            if let Some(local) = s.strip_prefix(r"\\?\") {
                return Ok(PathBuf::from(local));
            }
        }
    }
    Ok(canonical)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn move_to_trash(path: &Path) -> std::result::Result<(), String> {
    trash::delete(path).map_err(|e| e.to_string())
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn move_to_trash(_path: &Path) -> std::result::Result<(), String> {
    Err("trash is not supported on this platform".into())
}
