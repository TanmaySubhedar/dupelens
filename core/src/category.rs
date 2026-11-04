//! Broad kinds of file, for the UI's filter chips and file-type icons. Decided by file
//! extension only; nothing is read from the file.

use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Photo,
    Video,
    Music,
    Document,
    Archive,
    Other,
}

const PHOTO: &[&str] = &[
    "jpg", "jpeg", "jpe", "png", "webp", "gif", "bmp", "tif", "tiff", "heic", "heif", "avif",
    "raw", "cr2", "cr3", "nef", "arw", "dng", "orf", "rw2", "svg",
];
const VIDEO: &[&str] = &[
    "mp4", "mov", "mkv", "avi", "wmv", "flv", "webm", "m4v", "mpg", "mpeg", "3gp", "mts", "m2ts",
];
const MUSIC: &[&str] = &[
    "mp3", "wav", "flac", "aac", "m4a", "ogg", "opus", "wma", "aiff", "alac",
];
const DOCUMENT: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "txt", "rtf", "md",
    "csv", "epub", "pages", "numbers", "key",
];
const ARCHIVE: &[&str] = &[
    "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "tgz", "iso", "dmg",
];

impl Category {
    pub fn from_path(path: &Path) -> Self {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return Self::Other;
        };
        let is = |list: &[&str]| list.iter().any(|x| x.eq_ignore_ascii_case(ext));
        if is(PHOTO) {
            Self::Photo
        } else if is(VIDEO) {
            Self::Video
        } else if is(MUSIC) {
            Self::Music
        } else if is(DOCUMENT) {
            Self::Document
        } else if is(ARCHIVE) {
            Self::Archive
        } else {
            Self::Other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_map_to_categories_ignoring_case() {
        let c = |p: &str| Category::from_path(Path::new(p));
        assert_eq!(c("/a/IMG_1.JPG"), Category::Photo);
        assert_eq!(c("clip.MkV"), Category::Video);
        assert_eq!(c("song.flac"), Category::Music);
        assert_eq!(c("report.PDF"), Category::Document);
        assert_eq!(c("backup.zip"), Category::Archive);
        assert_eq!(c("data.bin"), Category::Other);
        assert_eq!(c("Makefile"), Category::Other);
    }
}
