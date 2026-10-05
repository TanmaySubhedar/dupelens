use serde::Serialize;
use std::path::PathBuf;

/// Everything the engine knows about one scanned file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileRecord {
    pub id: i64,
    pub path: PathBuf,
    pub size: u64,
    /// Last-modified time in nanoseconds since the Unix epoch. Used for incremental rescans.
    pub mtime_ns: i64,
    /// Lowercase hex SHA-256 of the file's bytes.
    pub sha256: String,
    /// Perceptual hashes; `None` when the format cannot be decoded (e.g. HEIC).
    pub ahash: Option<u64>,
    pub dhash: Option<u64>,
    pub phash: Option<u64>,
    /// Pixel dimensions after applying EXIF orientation.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Variance of the Laplacian on a normalised thumbnail; higher means sharper.
    pub sharpness: Option<f64>,
    pub exif_tags: u32,
    /// EXIF original capture time, formatted `YYYY-MM-DD HH:MM:SS`.
    pub taken_at: Option<String>,
    pub favorite: bool,
}

impl FileRecord {
    pub fn pixels(&self) -> Option<u64> {
        Some(self.width? as u64 * self.height? as u64)
    }

    pub fn longest_side(&self) -> Option<u32> {
        Some(self.width?.max(self.height?))
    }

    pub fn aspect_ratio(&self) -> Option<f64> {
        let (w, h) = (self.width?, self.height?);
        (h > 0).then(|| w as f64 / h as f64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupKind {
    /// Every member is byte-for-byte identical (same SHA-256).
    Exact,
    /// At least one member differs in bytes but looks the same (perceptual hash match).
    Near,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupMember {
    pub file: FileRecord,
    /// Same SHA-256 as the recommended file.
    pub identical_to_recommended: bool,
    /// pHash Hamming distance to the recommended file (0 = visually identical).
    pub distance_to_recommended: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    /// Stable for a given set of files: the smallest member file id.
    pub id: i64,
    pub kind: GroupKind,
    pub members: Vec<GroupMember>,
    /// File id of the copy the engine suggests keeping. Advisory only.
    pub recommended_id: i64,
    /// Plain-language reason for the recommendation.
    pub reason: String,
    /// Bytes freed if every non-recommended member is deleted.
    pub reclaimable_bytes: u64,
}

impl DuplicateGroup {
    /// The default "Keep Recommended" selection: every member except the recommended one.
    pub fn default_deletion(&self) -> Vec<i64> {
        self.members
            .iter()
            .map(|m| m.file.id)
            .filter(|&id| id != self.recommended_id)
            .collect()
    }
}
