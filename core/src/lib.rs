//! DupeLens core engine: offline duplicate photo detection.
//!
//! Nothing here touches the network. Files are only opened for reading until the user
//! explicitly asks [`Engine::delete`] to remove specific files.

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

use rayon::prelude::*;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

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
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanSummary {
    pub files_found: usize,
    /// Files that were new or changed and therefore re-analysed.
    pub files_analyzed: usize,
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

pub struct Engine {
    conn: Connection,
    pub grouping: GroupingConfig,
}

impl Engine {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        Ok(Self {
            conn: db::open(db_path)?,
            grouping: GroupingConfig::default(),
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        Ok(Self {
            conn: db::open_in_memory()?,
            grouping: GroupingConfig::default(),
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
    pub fn scan(&mut self, progress: &(dyn Fn(ScanProgress) + Sync)) -> Result<ScanSummary> {
        let folders = self.folders()?;
        if folders.is_empty() {
            return Err(Error::NoFolders);
        }
        let mut summary = ScanSummary::default();
        let (present, missing): (Vec<_>, Vec<_>) = folders.into_iter().partition(|f| f.is_dir());
        summary.missing_folders = missing;

        progress(ScanProgress {
            phase: ScanPhase::Discovering,
            done: 0,
            total: 0,
        });
        let discovery = scanner::discover(&present);
        summary.files_found = discovery.files.len();
        summary.unreadable = discovery.skipped;

        let known = db::fingerprints(&self.conn)?;
        let (unchanged, changed): (Vec<_>, Vec<_>) = discovery.files.into_iter().partition(|f| {
            known
                .get(&f.path)
                .is_some_and(|k| k.size == f.size && k.mtime_ns == f.mtime_ns)
        });
        summary.files_unchanged = unchanged.len();

        let total = changed.len();
        let done = AtomicUsize::new(0);
        progress(ScanProgress {
            phase: ScanPhase::Analyzing,
            done: 0,
            total,
        });
        let analysed: Vec<_> = changed
            .into_par_iter()
            .map(|f| {
                let result = metadata::analyze(&f.path, f.size);
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                progress(ScanProgress {
                    phase: ScanPhase::Analyzing,
                    done: n,
                    total,
                });
                (f, result)
            })
            .collect();

        progress(ScanProgress {
            phase: ScanPhase::Saving,
            done: 0,
            total: 0,
        });
        let tx = self.conn.transaction()?;
        let scan_id = db::next_scan_id(&tx)?;
        for f in &unchanged {
            db::mark_seen(&tx, known[&f.path].id, scan_id)?;
        }
        for (f, result) in analysed {
            match result {
                Ok(a) => {
                    db::upsert(&tx, &f.path, f.size, f.mtime_ns, &a, scan_id)?;
                    summary.files_analyzed += 1;
                }
                Err(_) => summary.unreadable.push(f.path),
            }
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
        let computed: Vec<_> = missing
            .into_par_iter()
            .map(|(i, j, k)| {
                // Unreadable or undecodable right now (e.g. file moved): no score, no match.
                let score = match (
                    metadata::load_gray(&files[i].path),
                    metadata::load_gray(&files[j].path),
                ) {
                    (Ok(Some(a)), Ok(Some(b))) => Some(hashing::local_difference(&a, &b)),
                    _ => None,
                };
                (k, score)
            })
            .collect();
        for (k, score) in computed {
            if let Some(score) = score {
                db::store_local_difference(&self.conn, &k.0, &k.1, LOCAL_DIFFERENCE_METHOD, score)?;
                scores.insert(k, score);
            }
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
