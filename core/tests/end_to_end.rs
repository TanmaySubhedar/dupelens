//! End-to-end tests against real files on disk.

use dupelens_core::{DeleteMode, Engine, Error, GroupKind};
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use std::fs;
use std::path::{Path, PathBuf};

/// A photo-like picture: smooth gradients plus a few shapes, seeded so scenes differ.
fn scene(w: u32, h: u32, seed: u32) -> DynamicImage {
    let s = seed as f64;
    DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f64 / w as f64, y as f64 / h as f64);
        let cx = 0.3 + 0.4 * ((s * 1.7).sin() * 0.5 + 0.5);
        let cy = 0.3 + 0.4 * ((s * 2.3).cos() * 0.5 + 0.5);
        let disc = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() < 0.2;
        let band = ((fx * (3.0 + s) + fy * 2.0) * std::f64::consts::PI).sin() > 0.6;
        let base = (fx * 120.0 + fy * 80.0) as u8;
        match (disc, band) {
            (true, _) => Rgb([230, 200 - (s as u8 * 20), 40]),
            (false, true) => Rgb([base / 2, base, 160]),
            _ => Rgb([base, base / 2 + 40, 90]),
        }
    }))
}

fn save(img: &DynamicImage, path: &Path, format: ImageFormat) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    img.save_with_format(path, format).unwrap();
}

struct Library {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

/// photos/
///   original.png         4 x 3 scene A, 1200x900 (best copy)
///   copy-of-original.png byte-identical to original.png
///   sub/small.jpg        scene A resized to 600x450 and saved as JPEG (near duplicate)
///   unrelated.png        scene B
/// elsewhere/
///   not-selected.png     identical to original.png, but in a folder never selected
fn library() -> Library {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let a = scene(1200, 900, 1);
    save(&a, &root.join("photos/original.png"), ImageFormat::Png);
    fs::copy(
        root.join("photos/original.png"),
        root.join("photos/copy-of-original.png"),
    )
    .unwrap();
    save(
        &a.resize_exact(600, 450, image::imageops::FilterType::Lanczos3),
        &root.join("photos/sub/small.jpg"),
        ImageFormat::Jpeg,
    );
    save(
        &scene(1200, 900, 4),
        &root.join("photos/unrelated.png"),
        ImageFormat::Png,
    );
    fs::create_dir_all(root.join("elsewhere")).unwrap();
    fs::copy(
        root.join("photos/original.png"),
        root.join("elsewhere/not-selected.png"),
    )
    .unwrap();
    Library { _dir: dir, root }
}

fn engine_for(lib: &Library) -> Engine {
    let engine = Engine::open_in_memory().unwrap();
    engine.add_folder(&lib.root.join("photos")).unwrap();
    engine
}

fn name(p: &Path) -> String {
    p.file_name().unwrap().to_string_lossy().into_owned()
}

#[test]
fn scanning_without_folders_is_an_error() {
    let mut engine = Engine::open_in_memory().unwrap();
    assert!(matches!(engine.scan(&|_| {}), Err(Error::NoFolders)));
}

#[test]
fn finds_exact_and_near_duplicates_in_selected_folders_only() {
    let lib = library();
    let mut engine = engine_for(&lib);
    let summary = engine.scan(&|_| {}).unwrap();
    assert_eq!(summary.files_found, 4, "elsewhere/ must not be scanned");

    let groups = engine.groups().unwrap();
    assert_eq!(groups.len(), 1, "{groups:#?}");
    let g = &groups[0];
    assert_eq!(
        g.kind,
        GroupKind::Near,
        "group mixes identical and resized copies"
    );

    let mut names: Vec<String> = g.members.iter().map(|m| name(&m.file.path)).collect();
    names.sort();
    assert_eq!(names, ["copy-of-original.png", "original.png", "small.jpg"]);

    // Recommendation: a full-resolution copy, never the small JPEG.
    let keep = g
        .members
        .iter()
        .find(|m| m.file.id == g.recommended_id)
        .unwrap();
    assert_eq!(keep.file.width, Some(1200));
    assert!(g.reason.contains("1200"), "reason: {}", g.reason);
    assert_eq!(g.default_deletion().len(), 2);
    assert!(!g.default_deletion().contains(&g.recommended_id));
}

#[test]
fn rescans_are_incremental() {
    let lib = library();
    let mut engine = engine_for(&lib);
    engine.scan(&|_| {}).unwrap();

    let again = engine.scan(&|_| {}).unwrap();
    assert_eq!((again.files_analyzed, again.files_unchanged), (0, 4));

    fs::remove_file(lib.root.join("photos/unrelated.png")).unwrap();
    save(
        &scene(800, 600, 9),
        &lib.root.join("photos/new.png"),
        ImageFormat::Png,
    );
    let third = engine.scan(&|_| {}).unwrap();
    assert_eq!(third.files_analyzed, 1, "only the new file is analysed");
    assert_eq!(third.files_removed, 1, "the deleted file is forgotten");
}

#[test]
fn deselecting_a_folder_drops_its_files() {
    let lib = library();
    let mut engine = engine_for(&lib);
    engine.add_folder(&lib.root.join("elsewhere")).unwrap();
    engine.scan(&|_| {}).unwrap();
    assert_eq!(engine.groups().unwrap()[0].members.len(), 4);

    engine.remove_folder(&lib.root.join("elsewhere")).unwrap();
    engine.scan(&|_| {}).unwrap();
    assert_eq!(engine.groups().unwrap()[0].members.len(), 3);
}

#[test]
fn refuses_to_delete_every_copy_in_a_group() {
    let lib = library();
    let mut engine = engine_for(&lib);
    engine.scan(&|_| {}).unwrap();
    let g = &engine.groups().unwrap()[0];
    let all: Vec<i64> = g.members.iter().map(|m| m.file.id).collect();

    assert!(matches!(
        engine.delete(&all, DeleteMode::Permanent),
        Err(Error::DeletionRefused(_))
    ));
    for m in &g.members {
        assert!(
            m.file.path.exists(),
            "nothing may be deleted when the request is refused"
        );
    }
}

#[test]
fn refuses_unknown_file_ids() {
    let lib = library();
    let mut engine = engine_for(&lib);
    engine.scan(&|_| {}).unwrap();
    assert!(matches!(
        engine.delete(&[999_999], DeleteMode::Permanent),
        Err(Error::DeletionRefused(_))
    ));
}

#[test]
fn skips_files_changed_since_the_scan() {
    let lib = library();
    let mut engine = engine_for(&lib);
    engine.scan(&|_| {}).unwrap();
    let g = &engine.groups().unwrap()[0];
    let victim = g
        .members
        .iter()
        .find(|m| name(&m.file.path) == "small.jpg")
        .unwrap();

    // The user edits the photo after scanning: it is no longer what they reviewed.
    fs::write(&victim.file.path, b"edited since scan").unwrap();
    let report = engine
        .delete(&[victim.file.id], DeleteMode::Permanent)
        .unwrap();
    assert!(report.deleted.is_empty());
    assert_eq!(report.failed.len(), 1);
    assert!(victim.file.path.exists());
}

#[test]
fn permanent_delete_removes_files_and_frees_space() {
    let lib = library();
    let mut engine = engine_for(&lib);
    engine.scan(&|_| {}).unwrap();
    let g = engine.groups().unwrap().remove(0);
    let ids = g.default_deletion();

    let report = engine.delete(&ids, DeleteMode::Permanent).unwrap();
    assert_eq!(report.deleted.len(), 2, "{:?}", report.failed);
    assert_eq!(report.bytes_freed, g.reclaimable_bytes);
    for m in &g.members {
        assert_eq!(m.file.path.exists(), m.file.id == g.recommended_id);
    }
    assert!(engine.groups().unwrap().is_empty(), "group resolved");
}

#[test]
fn trash_delete_moves_file_out_of_the_folder() {
    if !dupelens_core::trash_supported() {
        return;
    }
    let lib = library();
    let mut engine = engine_for(&lib);
    engine.scan(&|_| {}).unwrap();
    let g = &engine.groups().unwrap()[0];
    let victim = g
        .members
        .iter()
        .find(|m| name(&m.file.path) == "copy-of-original.png")
        .unwrap();

    let report = engine.delete(&[victim.file.id], DeleteMode::Trash).unwrap();
    assert_eq!(report.deleted, vec![victim.file.id], "{:?}", report.failed);
    assert!(!victim.file.path.exists());
}

#[test]
fn persists_folders_and_results_across_restarts() {
    let lib = library();
    let db = lib.root.join("state/dupelens.db");
    {
        let mut engine = Engine::open(&db).unwrap();
        engine.add_folder(&lib.root.join("photos")).unwrap();
        engine.scan(&|_| {}).unwrap();
    }
    let mut engine = Engine::open(&db).unwrap();
    assert_eq!(engine.folders().unwrap().len(), 1);
    assert_eq!(engine.groups().unwrap().len(), 1);
    assert_eq!(engine.scan(&|_| {}).unwrap().files_unchanged, 4);
}

#[test]
fn same_folder_under_different_spellings_is_stored_once() {
    let lib = library();
    let mut engine = Engine::open_in_memory().unwrap();
    let photos = lib.root.join("photos");
    engine.add_folder(&photos).unwrap();
    engine.add_folder(&lib.root.join("photos/sub/..")).unwrap();
    if cfg!(windows) {
        let shouty = PathBuf::from(photos.to_str().unwrap().to_uppercase());
        engine.add_folder(&shouty).unwrap();
    }
    assert_eq!(engine.folders().unwrap().len(), 1);

    // Otherwise each photo would be scanned twice and look like a duplicate of itself.
    engine.scan(&|_| {}).unwrap();
    let g = &engine.groups().unwrap()[0];
    assert_eq!(g.members.len(), 3);
}

/// An app-window-like screen: header bar, dark sidebar, side panel and lines of "text".
/// `variant` 1 rewrites a short stretch of one line, like a ticking timer.
fn screen(variant: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(1600, 1000, |x, y| {
        if y < 80 {
            return Rgb([40, 60, 120]); // header bar
        }
        if x < 250 {
            return Rgb([30, 30, 35]); // sidebar
        }
        if (1150..1550).contains(&x) && (150..850).contains(&y) {
            return Rgb([200, 210, 225]); // side panel
        }
        let line = y / 40;
        let in_text = y % 40 < 16 && (320..1050).contains(&x);
        let ink = match (line, variant) {
            (12, 1) if x < 470 => (x / 23) % 3 != 0,
            _ => (x / 17 + line) % 4 != 0,
        };
        if in_text && ink {
            Rgb([40, 40, 40])
        } else {
            Rgb([245, 245, 240])
        }
    }))
}

#[test]
fn screenshots_with_a_changed_line_are_not_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("shots");
    save(&screen(0), &root.join("before.png"), ImageFormat::Png);
    save(&screen(1), &root.join("after.png"), ImageFormat::Png);
    save(
        &screen(0).resize_exact(800, 500, image::imageops::FilterType::Lanczos3),
        &root.join("before-small.jpg"),
        ImageFormat::Jpeg,
    );

    // The hashes alone cannot tell them apart; that is what the pixel check is for.
    let analyse = |n: &str| {
        let p = root.join(n);
        dupelens_core::metadata::analyze(&p, fs::metadata(&p).unwrap().len()).unwrap()
    };
    let (a, b) = (analyse("before.png"), analyse("after.png"));
    let d = dupelens_core::hashing::hamming(a.hashes.unwrap().phash, b.hashes.unwrap().phash);
    assert!(
        d <= 8,
        "test needs a pHash match to be meaningful (distance {d})"
    );

    let mut engine = Engine::open_in_memory().unwrap();
    engine.add_folder(&root).unwrap();
    engine.scan(&|_| {}).unwrap();
    let groups = engine.groups().unwrap();
    assert_eq!(groups.len(), 1, "{groups:#?}");
    let mut names: Vec<String> = groups[0]
        .members
        .iter()
        .map(|m| name(&m.file.path))
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["before-small.jpg", "before.png"],
        "resized copy kept, edited screen excluded"
    );
}

#[test]
fn shrunken_copy_of_a_small_capture_is_still_a_duplicate() {
    // A partial-window capture (small) full of fine text, and a quarter-size low-quality copy.
    // Comparing them at a resolution above the copy's own would blow its smudged text up
    // against the crisp original and wrongly reject the match.
    let capture = DynamicImage::ImageRgb8(RgbImage::from_fn(720, 400, |x, y| {
        if x < 90 {
            return Rgb([30, 30, 35]);
        }
        let in_text = y % 14 < 7 && (110..650).contains(&x) && y > 30;
        if in_text && (x / 3 + y / 7) % 3 != 0 {
            Rgb([30, 30, 30])
        } else {
            Rgb([250, 250, 250])
        }
    }));
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("shots");
    save(&capture, &root.join("capture.png"), ImageFormat::Png);
    let tiny = capture
        .resize_exact(180, 100, image::imageops::FilterType::Triangle)
        .to_rgb8();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 40)
        .encode_image(&tiny)
        .unwrap();
    fs::write(root.join("capture-tiny.jpg"), jpeg).unwrap();

    let mut engine = Engine::open_in_memory().unwrap();
    engine.add_folder(&root).unwrap();
    engine.scan(&|_| {}).unwrap();
    let groups = engine.groups().unwrap();
    assert_eq!(groups.len(), 1, "quarter-size copy must be found");
    assert_eq!(groups[0].members.len(), 2);
}

// ---------------------------------------------------------------------------------------------
// Files that are not photos: exact matching only, read as little as possible.
// ---------------------------------------------------------------------------------------------

/// `n` bytes of deterministic filler that differs per `seed`.
fn filler(n: usize, seed: u8) -> Vec<u8> {
    (0..n)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn engine_with_folder(root: &Path) -> Engine {
    let engine = Engine::open_in_memory().unwrap();
    engine.add_folder(root).unwrap();
    engine
}

#[test]
fn identical_files_of_any_type_are_exact_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let video = filler(400_000, 1);
    write(&root.join("Videos/clip.mkv"), &video);
    write(&root.join("Downloads/clip.mkv"), &video);
    let report = filler(5_000, 2);
    write(&root.join("Documents/report.pdf"), &report);
    write(&root.join("Desktop/report (1).pdf"), &report);
    write(&root.join("Music/song.mp3"), &filler(3_000, 3)); // unique size and content

    let mut engine = engine_with_folder(root);
    let summary = engine.scan(&|_| {}).unwrap();
    assert_eq!(summary.files_found, 5);
    assert_eq!(
        summary.files_analyzed, 4,
        "only the four twins are read in full"
    );
    assert_eq!(summary.bytes_read, 2 * 400_000 + 2 * 5_000);
    assert_eq!(
        summary.files_without_twin, 1,
        "the song has no possible twin"
    );

    let groups = engine.groups().unwrap();
    assert_eq!(groups.len(), 2, "{groups:#?}");
    assert!(groups.iter().all(|g| g.kind == GroupKind::Exact));
    // Largest reclaimable space first.
    assert_eq!(groups[0].reclaimable_bytes, 400_000);

    // For the PDFs, the original name is kept and the "(1)" copy is suggested for deletion.
    let pdf = groups
        .iter()
        .find(|g| g.reclaimable_bytes == 5_000)
        .unwrap();
    let keep = pdf
        .members
        .iter()
        .find(|m| m.file.id == pdf.recommended_id)
        .unwrap();
    assert_eq!(name(&keep.file.path), "report.pdf");
}

#[test]
fn same_size_different_content_is_not_a_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // Differ at the very start: told apart by the cheap fingerprint, never read in full.
    write(&root.join("a.bin"), &filler(300_000, 1));
    write(&root.join("b.bin"), &filler(300_000, 2));
    // Differ only in the middle: the fingerprint matches, so the full hash must decide.
    let mut m1 = filler(300_000, 5);
    let mut m2 = m1.clone();
    m1[150_000] = 1;
    m2[150_000] = 2;
    write(&root.join("c.bin"), &m1);
    write(&root.join("d.bin"), &m2);

    let mut engine = engine_with_folder(root);
    let summary = engine.scan(&|_| {}).unwrap();
    // a.bin, b.bin, c.bin and d.bin share one size, so all four are fingerprinted; only c and d
    // match each other on it and so are read in full.
    assert_eq!(summary.files_analyzed, 2);
    assert_eq!(summary.files_without_twin, 2);
    assert!(engine.groups().unwrap().is_empty());
}

#[test]
fn a_file_with_no_twin_is_hashed_once_a_twin_appears() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let data = filler(80_000, 9);
    write(&root.join("original.zip"), &data);

    let mut engine = engine_with_folder(root);
    let first = engine.scan(&|_| {}).unwrap();
    assert_eq!((first.files_analyzed, first.files_without_twin), (0, 1));
    assert!(engine.groups().unwrap().is_empty());

    // Nothing changed: still nothing is read.
    let again = engine.scan(&|_| {}).unwrap();
    assert_eq!((again.files_analyzed, again.files_unchanged), (0, 1));

    // A copy arrives. The untouched original now needs a hash too.
    write(&root.join("backup/original.zip"), &data);
    let third = engine.scan(&|_| {}).unwrap();
    assert_eq!(third.files_unchanged, 1);
    assert_eq!(third.files_analyzed, 2);
    let groups = engine.groups().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].members.len(), 2);

    // And once hashed, an unchanged pair is not read again.
    let fourth = engine.scan(&|_| {}).unwrap();
    assert_eq!((fourth.files_analyzed, fourth.files_unchanged), (0, 2));
    assert_eq!(engine.groups().unwrap().len(), 1);
}

#[test]
fn a_non_photo_copy_of_a_photo_is_an_exact_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    save(
        &scene(300, 200, 2),
        &root.join("pics/holiday.png"),
        ImageFormat::Png,
    );
    fs::copy(root.join("pics/holiday.png"), root.join("pics/holiday.bak")).unwrap();

    let mut engine = engine_with_folder(root);
    engine.scan(&|_| {}).unwrap();
    let groups = engine.groups().unwrap();
    assert_eq!(groups.len(), 1, "{groups:#?}");
    assert_eq!(groups[0].members.len(), 2);
}

#[test]
fn deleting_a_non_photo_duplicate_works_and_keeps_the_other() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let data = filler(10_000, 4);
    write(&root.join("a/notes.docx"), &data);
    write(&root.join("b/notes.docx"), &data);

    let mut engine = engine_with_folder(root);
    engine.scan(&|_| {}).unwrap();
    let groups = engine.groups().unwrap();
    let ids = groups[0].default_deletion();
    let report = engine.delete(&ids, DeleteMode::Permanent).unwrap();
    assert_eq!(report.bytes_freed, 10_000);
    let remaining = [root.join("a/notes.docx"), root.join("b/notes.docx")]
        .iter()
        .filter(|p| p.exists())
        .count();
    assert_eq!(remaining, 1);
}

#[test]
fn an_old_database_is_upgraded_without_losing_folders() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("old.db");
    {
        // A version-1 database: hashes were required, and the user had picked a folder.
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE folders (path TEXT PRIMARY KEY, added_at INTEGER NOT NULL);
             CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE,
                 size INTEGER NOT NULL, mtime_ns INTEGER NOT NULL, sha256 TEXT NOT NULL,
                 scan_id INTEGER NOT NULL);
             INSERT INTO folders VALUES ('/kept', 0);
             INSERT INTO files VALUES (1, '/kept/a.jpg', 1, 1, 'x', 1);
             PRAGMA user_version = 1;",
        )
        .unwrap();
    }
    let engine = Engine::open(&db).unwrap();
    assert_eq!(engine.folders().unwrap(), vec![PathBuf::from("/kept")]);
}

// ---------------------------------------------------------------------------------------------
// Cancelling, byte progress and hard links.
// ---------------------------------------------------------------------------------------------

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

#[test]
fn a_scan_cancelled_before_it_starts_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let data = filler(50_000, 1);
    write(&dir.path().join("a.bin"), &data);
    write(&dir.path().join("b.bin"), &data);
    let mut engine = engine_with_folder(dir.path());

    let cancel = AtomicBool::new(true);
    assert!(matches!(
        engine.scan_cancellable(&|_| {}, &cancel),
        Err(Error::Cancelled)
    ));
    assert!(engine.groups().unwrap().is_empty(), "nothing was saved");

    // The same engine scans normally afterwards.
    engine.scan(&|_| {}).unwrap();
    assert_eq!(engine.groups().unwrap().len(), 1);
}

#[test]
fn cancelling_while_reading_keeps_the_database_usable() {
    let dir = tempfile::tempdir().unwrap();
    // Several big identical pairs, so there is real work to interrupt.
    for i in 0..6u8 {
        let data = filler(3_000_000, i);
        write(&dir.path().join(format!("a/{i}.bin")), &data);
        write(&dir.path().join(format!("b/{i}.bin")), &data);
    }
    let mut engine = engine_with_folder(dir.path());

    let cancel = AtomicBool::new(false);
    let result = engine.scan_cancellable(
        &|p| {
            if p.phase == dupelens_core::ScanPhase::Analyzing && p.bytes_done > 0 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    );
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");

    // A complete scan afterwards still finds every pair.
    engine.scan(&|_| {}).unwrap();
    assert_eq!(engine.groups().unwrap().len(), 6);
}

#[test]
fn progress_reports_bytes_to_read_and_read() {
    let dir = tempfile::tempdir().unwrap();
    let data = filler(2_500_000, 3);
    write(&dir.path().join("x/clip.bin"), &data);
    write(&dir.path().join("y/clip.bin"), &data);
    write(&dir.path().join("only-one.bin"), &filler(123_457, 4)); // never read
    let mut engine = engine_with_folder(dir.path());

    let last = Mutex::new((0u64, 0u64));
    engine
        .scan(&|p| {
            if p.phase == dupelens_core::ScanPhase::Analyzing && p.bytes_total > 0 {
                *last.lock().unwrap() = (p.bytes_done, p.bytes_total);
            }
        })
        .unwrap();
    let (done, total) = *last.lock().unwrap();
    assert_eq!(total, 5_000_000, "only the twins are to be read");
    assert_eq!(done, total, "the bar reaches the end");
}

#[test]
fn hard_links_are_not_reported_as_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(&root.join("one/report.pdf"), &filler(9_000, 7));
    fs::create_dir_all(root.join("two")).unwrap();
    fs::hard_link(root.join("one/report.pdf"), root.join("two/report.pdf")).unwrap();

    let mut engine = engine_with_folder(root);
    let summary = engine.scan(&|_| {}).unwrap();
    assert_eq!(summary.hard_links_skipped, 1);
    assert_eq!(summary.files_found, 1);
    assert!(engine.groups().unwrap().is_empty());

    // A genuine copy next to them is still found.
    write(&root.join("three/report.pdf"), &filler(9_000, 7));
    engine.scan(&|_| {}).unwrap();
    let groups = engine.groups().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].members.len(), 2);
}

#[test]
fn a_cancelled_comparison_returns_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let a = scene(400, 300, 3);
    save(&a, &dir.path().join("a.png"), ImageFormat::Png);
    save(
        &a.resize_exact(200, 150, image::imageops::FilterType::Lanczos3),
        &dir.path().join("b.jpg"),
        ImageFormat::Jpeg,
    );
    let mut engine = engine_with_folder(dir.path());
    engine.scan(&|_| {}).unwrap();

    let cancel = AtomicBool::new(true);
    assert!(matches!(
        engine.groups_cancellable(&|_, _| {}, &cancel),
        Err(Error::Cancelled)
    ));
    // Not cancelled: finds the pair, and reports its progress.
    let steps = Mutex::new(Vec::new());
    let groups = engine
        .groups_cancellable(
            &|d, t| steps.lock().unwrap().push((d, t)),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(steps.lock().unwrap().last(), Some(&(1, 1)));
}

// ---------------------------------------------------------------------------------------------
// Matching settings.
// ---------------------------------------------------------------------------------------------

use dupelens_core::{GroupingConfig, MatchingSettings, Preset, ScanOptions};

#[test]
fn changing_the_preset_regroups_without_rescanning() {
    let lib = library(); // original.png, an exact copy, and a resized JPEG of it
    let mut engine = engine_for(&lib);
    engine.scan(&|_| {}).unwrap();

    let members = |engine: &Engine| {
        let groups = engine.groups().unwrap();
        (groups.len(), groups.first().map_or(0, |g| g.members.len()))
    };
    assert_eq!(members(&engine), (1, 3), "Balanced: all three");

    engine
        .set_matching(MatchingSettings::preset(Preset::ExactOnly))
        .unwrap();
    assert_eq!(
        members(&engine),
        (1, 2),
        "Exact only: the resized copy is left out"
    );
    assert_eq!(engine.groups().unwrap()[0].kind, GroupKind::Exact);

    engine
        .set_matching(MatchingSettings::preset(Preset::Balanced))
        .unwrap();
    assert_eq!(
        members(&engine),
        (1, 3),
        "back to Balanced, no rescan in between"
    );

    // A rescan after all that still reads nothing.
    let again = engine.scan(&|_| {}).unwrap();
    assert_eq!(again.files_analyzed, 0);
}

#[test]
fn settings_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("app.db");
    {
        let mut engine = Engine::open(&db).unwrap();
        assert_eq!(engine.matching().preset, Preset::Balanced, "default");
        engine
            .set_matching(
                MatchingSettings::custom(GroupingConfig {
                    phash_max: 5,
                    dhash_max: 7,
                    aspect_tolerance: 0.04,
                    max_local_difference: 12.5,
                    ..GroupingConfig::default()
                })
                .unwrap(),
            )
            .unwrap();
        engine
            .set_scan_options(ScanOptions { min_size: 4096 })
            .unwrap();
    }
    let engine = Engine::open(&db).unwrap();
    let m = engine.matching();
    assert_eq!(m.preset, Preset::Custom);
    assert_eq!(
        (
            m.config.phash_max,
            m.config.dhash_max,
            m.config.aspect_tolerance,
            m.config.max_local_difference
        ),
        (5, 7, 0.04, 12.5)
    );
    assert_eq!(engine.scan_options.min_size, 4096);

    let mut engine = engine;
    engine
        .set_matching(MatchingSettings::preset(Preset::Loose))
        .unwrap();
    drop(engine);
    assert_eq!(Engine::open(&db).unwrap().matching().preset, Preset::Loose);
}

#[test]
fn a_minimum_size_leaves_small_files_out() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("a/tiny.txt"), b"hello");
    write(&dir.path().join("b/tiny.txt"), b"hello");
    let mut engine = engine_with_folder(dir.path());
    engine
        .set_scan_options(ScanOptions { min_size: 1024 })
        .unwrap();
    engine.scan(&|_| {}).unwrap();
    assert!(engine.groups().unwrap().is_empty());
    engine
        .set_scan_options(ScanOptions { min_size: 1 })
        .unwrap();
    engine.scan(&|_| {}).unwrap();
    assert_eq!(engine.groups().unwrap().len(), 1);
}
