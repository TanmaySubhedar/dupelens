//! The API the Flutter app calls. flutter_rust_bridge generates matching Dart code from
//! the public items in this module. Keep it thin: logic belongs in dupelens-core.

use crate::frb_generated::StreamSink;
use dupelens_core::{DeleteMode, DuplicateGroup, Engine, GroupKind, ScanPhase};
use flutter_rust_bridge::frb;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct FileDto {
    pub id: i64,
    pub path: String,
    pub size_bytes: i64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub taken_at: Option<String>,
    pub favorite: bool,
    pub identical_to_recommended: bool,
    /// 0 = visually identical; larger = less similar (max 64).
    pub distance_to_recommended: Option<u32>,
}

pub enum GroupKindDto {
    Exact,
    Near,
}

pub struct GroupDto {
    pub id: i64,
    pub kind: GroupKindDto,
    pub members: Vec<FileDto>,
    pub recommended_id: i64,
    pub reason: String,
    pub reclaimable_bytes: i64,
}

pub enum ScanPhaseDto {
    Discovering,
    Analyzing,
    Saving,
}

pub struct ScanSummaryDto {
    pub files_found: u32,
    pub files_analyzed: u32,
    pub files_unchanged: u32,
    pub files_removed: u32,
    pub unreadable_count: u32,
    pub missing_folders: Vec<String>,
}

pub enum ScanEvent {
    Progress { phase: ScanPhaseDto, done: u32, total: u32 },
    Finished(ScanSummaryDto),
}

pub struct DeleteReportDto {
    pub deleted_ids: Vec<i64>,
    pub failed: Vec<DeleteFailureDto>,
    pub bytes_freed: i64,
}

pub struct DeleteFailureDto {
    pub file_id: i64,
    pub reason: String,
}

fn path_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

impl From<DuplicateGroup> for GroupDto {
    fn from(g: DuplicateGroup) -> Self {
        GroupDto {
            id: g.id,
            kind: match g.kind {
                GroupKind::Exact => GroupKindDto::Exact,
                GroupKind::Near => GroupKindDto::Near,
            },
            members: g
                .members
                .into_iter()
                .map(|m| FileDto {
                    id: m.file.id,
                    path: path_string(&m.file.path),
                    size_bytes: m.file.size as i64,
                    width: m.file.width,
                    height: m.file.height,
                    taken_at: m.file.taken_at,
                    favorite: m.file.favorite,
                    identical_to_recommended: m.identical_to_recommended,
                    distance_to_recommended: m.distance_to_recommended,
                })
                .collect(),
            recommended_id: g.recommended_id,
            reason: g.reason,
            reclaimable_bytes: g.reclaimable_bytes as i64,
        }
    }
}

/// One open engine (database + settings). Dart holds it as an opaque handle.
#[frb(opaque)]
pub struct DupeLens {
    engine: Mutex<Engine>,
}

impl DupeLens {
    /// Opens (or creates) the database at `db_path`.
    #[frb(sync)]
    pub fn open(db_path: String) -> anyhow::Result<DupeLens> {
        Ok(DupeLens {
            engine: Mutex::new(Engine::open(Path::new(&db_path))?),
        })
    }

    #[frb(sync)]
    pub fn trash_supported() -> bool {
        dupelens_core::trash_supported()
    }

    fn engine(&self) -> std::sync::MutexGuard<'_, Engine> {
        // A panic while holding the lock leaves the engine usable; recover the guard.
        self.engine.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn folders(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.engine().folders()?.iter().map(|p| path_string(p)).collect())
    }

    pub fn add_folder(&self, path: String) -> anyhow::Result<String> {
        Ok(path_string(&self.engine().add_folder(Path::new(&path))?))
    }

    pub fn remove_folder(&self, path: String) -> anyhow::Result<bool> {
        Ok(self.engine().remove_folder(Path::new(&path))?)
    }

    /// Scans the selected folders, streaming progress (at most ~10 updates per second)
    /// and finishing with a summary.
    pub fn scan(&self, sink: StreamSink<ScanEvent>) -> anyhow::Result<()> {
        let last = Mutex::new(Instant::now() - Duration::from_secs(1));
        let summary = self.engine().scan(&|p| {
            let finished_phase = p.total > 0 && p.done == p.total;
            let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
            if p.done == 0 || finished_phase || last.elapsed() >= Duration::from_millis(100) {
                *last = Instant::now();
                let phase = match p.phase {
                    ScanPhase::Discovering => ScanPhaseDto::Discovering,
                    ScanPhase::Analyzing => ScanPhaseDto::Analyzing,
                    ScanPhase::Saving => ScanPhaseDto::Saving,
                };
                // The Dart side may have stopped listening; that is not an error here.
                let _ = sink.add(ScanEvent::Progress {
                    phase,
                    done: p.done as u32,
                    total: p.total as u32,
                });
            }
        })?;
        let _ = sink.add(ScanEvent::Finished(ScanSummaryDto {
            files_found: summary.files_found as u32,
            files_analyzed: summary.files_analyzed as u32,
            files_unchanged: summary.files_unchanged as u32,
            files_removed: summary.files_removed as u32,
            unreadable_count: summary.unreadable.len() as u32,
            missing_folders: summary.missing_folders.iter().map(|p| path_string(p)).collect(),
        }));
        Ok(())
    }

    pub fn groups(&self) -> anyhow::Result<Vec<GroupDto>> {
        Ok(self.engine().groups()?.into_iter().map(GroupDto::from).collect())
    }

    /// Deletes exactly these files. Only call after the user has confirmed.
    pub fn delete(&self, file_ids: Vec<i64>, permanent: bool) -> anyhow::Result<DeleteReportDto> {
        let mode = if permanent { DeleteMode::Permanent } else { DeleteMode::Trash };
        let report = self.engine().delete(&file_ids, mode)?;
        Ok(DeleteReportDto {
            deleted_ids: report.deleted,
            failed: report
                .failed
                .into_iter()
                .map(|(file_id, reason)| DeleteFailureDto { file_id, reason })
                .collect(),
            bytes_freed: report.bytes_freed as i64,
        })
    }
}
