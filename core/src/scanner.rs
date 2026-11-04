//! Finds candidate files inside the user's selected folders, and nowhere else.
//!
//! Every regular file is a candidate, whatever its type. Photos get the full look-alike
//! analysis; everything else takes part in exact matching only (see [`crate::Engine::scan`]).

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

/// Photos: these get perceptual hashing on top of exact matching.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "png", "webp", "gif", "bmp", "tif", "tiff", "heic", "heif", "avif",
];

/// Formats we can decode for perceptual hashing. The rest (HEIC/HEIF/AVIF) get exact matching only.
const DECODABLE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "png", "webp", "gif", "bmp", "tif", "tiff",
];

/// Folder names that hold OS bookkeeping, installed programs, application data or
/// dependency caches. Files in them are not the user's to tidy, and "deleting the duplicate"
/// there can break an installed program. Matched by name at any depth below a selected folder;
/// a folder the user selects directly is always scanned.
const SKIPPED_DIRS: &[&str] = &[
    "$recycle.bin",
    "system volume information",
    "@eadir",
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "appdata",
    "node_modules",
];

/// Files that are OS bookkeeping, not user content.
const SKIPPED_FILES: &[&str] = &["thumbs.db", "desktop.ini", "ehthumbs.db"];

/// What to include in a scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanOptions {
    /// Files smaller than this many bytes are ignored. The default of 1 skips empty files,
    /// which are all "identical" and never worth reviewing.
    pub min_size: u64,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self { min_size: 1 }
    }
}

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
    /// Files that exist only in the cloud (e.g. OneDrive "Files On-Demand"). Reading one would
    /// download it, so they are never opened.
    pub cloud_only: usize,
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

/// Dotfiles (`.DS_Store`), Office lock files (`~$report.docx`) and OS bookkeeping files.
fn is_skipped_file(name: &str) -> bool {
    name.starts_with('.')
        || name.starts_with("~$")
        || SKIPPED_FILES.iter().any(|s| s.eq_ignore_ascii_case(name))
}

/// A Windows file whose content is not on this computer (OneDrive "Files On-Demand" and
/// similar). Reading it would download it. Always false elsewhere.
#[cfg(windows)]
fn is_cloud_placeholder(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_OPEN: u32 = 0x0004_0000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    meta.file_attributes() & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0
}

#[cfg(not(windows))]
fn is_cloud_placeholder(_meta: &std::fs::Metadata) -> bool {
    false
}

/// A Windows file flagged as an OS system file. Always false elsewhere.
#[cfg(windows)]
fn is_system_file(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const SYSTEM: u32 = 0x4;
    meta.file_attributes() & SYSTEM != 0
}

#[cfg(not(windows))]
fn is_system_file(_meta: &std::fs::Metadata) -> bool {
    false
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

/// Walks the given folders (never following symbolic links out of them) and lists their files.
pub fn discover(folders: &[PathBuf], options: &ScanOptions) -> Discovery {
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
            if !entry.file_type().is_file()
                || entry.file_name().to_str().is_some_and(is_skipped_file)
            {
                continue;
            }
            // Paths are stored as text; a lossy conversion could later point deletion at the wrong file.
            if entry.path().to_str().is_none() {
                out.skipped.push(entry.path().to_path_buf());
                continue;
            }
            match entry.metadata() {
                Ok(meta) => {
                    if is_system_file(&meta) || meta.len() < options.min_size {
                        continue;
                    }
                    if is_cloud_placeholder(&meta) {
                        out.cloud_only += 1;
                        continue;
                    }
                    out.files.push(DiscoveredFile {
                        path: entry.path().to_path_buf(),
                        size: meta.len(),
                        mtime_ns: mtime_ns(&meta),
                    });
                }
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
    fn discovers_every_file_type_and_skips_hidden_and_system_places() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::create_dir_all(root.join(".thumbnails")).unwrap();
        fs::write(root.join("a.JPG"), b"x").unwrap();
        fs::write(root.join("sub/b.png"), b"x").unwrap();
        fs::write(root.join("notes.txt"), b"x").unwrap();
        fs::write(root.join("movie.mkv"), b"x").unwrap();
        fs::write(root.join(".thumbnails/c.jpg"), b"x").unwrap();
        fs::write(root.join(".DS_Store"), b"x").unwrap();
        fs::write(root.join("~$report.docx"), b"x").unwrap();
        fs::write(root.join("Thumbs.db"), b"x").unwrap();
        fs::write(root.join("empty.bin"), b"").unwrap();
        fs::create_dir_all(root.join("project/node_modules/dep")).unwrap();
        fs::write(root.join("project/node_modules/dep/index.js"), b"x").unwrap();
        fs::write(root.join("project/main.js"), b"x").unwrap();

        let names = |opts: &ScanOptions| {
            let mut n: Vec<String> = discover(&[root.to_path_buf()], opts)
                .files
                .iter()
                .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
                .collect();
            n.sort();
            n
        };
        assert_eq!(
            names(&ScanOptions::default()),
            vec!["a.JPG", "b.png", "main.js", "movie.mkv", "notes.txt"]
        );
        // Empty files come back only when the minimum size allows them.
        assert!(names(&ScanOptions { min_size: 0 }).contains(&"empty.bin".to_string()));
    }

    #[test]
    fn a_system_folder_is_scanned_when_selected_directly() {
        let dir = tempfile::tempdir().unwrap();
        let appdata = dir.path().join("AppData");
        fs::create_dir_all(&appdata).unwrap();
        fs::write(appdata.join("keep.txt"), b"x").unwrap();
        let found = discover(&[appdata], &ScanOptions::default()).files;
        assert_eq!(found.len(), 1);
    }
}
