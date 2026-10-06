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
