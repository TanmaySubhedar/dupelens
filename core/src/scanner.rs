//! Finds candidate files inside the user's selected folders, and nowhere else.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

/// Phase 1 scans images only.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "png", "webp", "gif", "bmp", "tif", "tiff", "heic", "heif", "avif",
];

/// Formats we can decode for perceptual hashing. The rest (HEIC/HEIF/AVIF) get exact matching only.
const DECODABLE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "png", "webp", "gif", "bmp", "tif", "tiff",
];

/// Folder names that hold OS bookkeeping, trash or generated thumbnails, never user photos.
const SKIPPED_DIRS: &[&str] = &["$recycle.bin", "system volume information", "@eadir"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    pub size: u64,
    pub mtime_ns: i64,
}

#[derive(Debug, Default)]
pub struct Discovery {
    pub files: Vec<DiscoveredFile>,
    /// Paths that could not be read (permissions etc.) or are not valid UTF-8.
    pub skipped: Vec<PathBuf>,
}

fn extension_in(path: &Path, list: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| list.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

pub fn is_image(path: &Path) -> bool {
    extension_in(path, IMAGE_EXTENSIONS)
}

pub fn is_decodable(path: &Path) -> bool {
    extension_in(path, DECODABLE_EXTENSIONS)
}

fn is_skipped_dir(name: &str) -> bool {
    // Hidden folders (".thumbnails", ".trash-1000", ".git") contain caches, not originals.
    name.starts_with('.') || SKIPPED_DIRS.iter().any(|s| s.eq_ignore_ascii_case(name))
}

/// Removes folders nested inside other selected folders so no file is visited twice.
pub fn collapse_nested(folders: &[PathBuf]) -> Vec<PathBuf> {
    let mut sorted: Vec<PathBuf> = folders.to_vec();
    sorted.sort();
    sorted.dedup();
    let mut roots: Vec<PathBuf> = Vec::new();
    for f in sorted {
        if !roots.iter().any(|r| f.starts_with(r)) {
            roots.push(f);
        }
    }
    roots
}

pub fn mtime_ns(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as i64)
}

/// Walks the given folders (never following symbolic links out of them) and lists image files.
pub fn discover(folders: &[PathBuf]) -> Discovery {
    let mut out = Discovery::default();
    for root in collapse_nested(folders) {
        let walker = WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                e.depth() == 0
                    || !e.file_type().is_dir()
                    || !e.file_name().to_str().is_some_and(is_skipped_dir)
            });
        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(err) => {
                    if let Some(p) = err.path() {
                        out.skipped.push(p.to_path_buf());
                    }
                    continue;
                }
            };
            if !entry.file_type().is_file() || !is_image(entry.path()) {
                continue;
            }
            // Paths are stored as text; a lossy conversion could later point deletion at the wrong file.
            if entry.path().to_str().is_none() {
                out.skipped.push(entry.path().to_path_buf());
                continue;
            }
            match entry.metadata() {
                Ok(meta) => out.files.push(DiscoveredFile {
                    path: entry.path().to_path_buf(),
                    size: meta.len(),
                    mtime_ns: mtime_ns(&meta),
                }),
                Err(_) => out.skipped.push(entry.path().to_path_buf()),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn nested_folders_collapse_to_their_parent() {
        let roots = collapse_nested(&[
            PathBuf::from("/a/b"),
            PathBuf::from("/a"),
            PathBuf::from("/ab"),
            PathBuf::from("/a"),
        ]);
        assert_eq!(roots, vec![PathBuf::from("/a"), PathBuf::from("/ab")]);
    }

    #[test]
    fn discovers_only_images_and_skips_hidden_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::create_dir_all(root.join(".thumbnails")).unwrap();
        fs::write(root.join("a.JPG"), b"x").unwrap();
        fs::write(root.join("sub/b.png"), b"x").unwrap();
        fs::write(root.join("notes.txt"), b"x").unwrap();
        fs::write(root.join(".thumbnails/c.jpg"), b"x").unwrap();

        let mut names: Vec<String> = discover(&[root.to_path_buf()])
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["a.JPG", "b.png"]);
    }
}
