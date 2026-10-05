//! Picks which copy in a duplicate group to keep. Advisory only: the user can override.
//!
//! Criteria are compared in the PRD's order; the first one that differs decides:
//! 1. Highest resolution (pixel count)
//! 2. Largest dimensions (longest side)
//! 3. Better image quality (sharpness, bucketed so tiny differences don't count)
//! 4. Richest EXIF metadata (tag count)
//! 5. Earliest original creation timestamp (EXIF)
//! 6. Favourite flag
//!
//! If everything ties (typical for byte-identical copies), the oldest file on disk wins,
//! then the shortest path, so the result is deterministic.

use crate::model::FileRecord;
use std::cmp::Ordering;

/// Sharpness within the same bucket (about 19% apart) counts as equal quality.
/// Bucketing (rather than "within X%") keeps the comparison transitive.
fn quality_bucket(f: &FileRecord) -> Option<i64> {
    f.sharpness
        .filter(|s| *s > 0.0)
        .map(|s| (s.log2() * 4.0).floor() as i64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Criterion {
    Resolution,
    Dimensions,
    Quality,
    Metadata,
    CreationDate,
    Favorite,
    Tiebreak,
}

/// `Greater` means `a` is the better copy to keep.
fn compare(a: &FileRecord, b: &FileRecord) -> (Ordering, Criterion) {
    use Criterion::*;
    let steps: [(Ordering, Criterion); 6] = [
        (a.pixels().cmp(&b.pixels()), Resolution),
        (a.longest_side().cmp(&b.longest_side()), Dimensions),
        (quality_bucket(a).cmp(&quality_bucket(b)), Quality),
        (a.exif_tags.cmp(&b.exif_tags), Metadata),
        // Earlier is better; a known date beats an unknown one.
        (
            match (&a.taken_at, &b.taken_at) {
                (Some(x), Some(y)) => y.cmp(x),
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            },
            CreationDate,
        ),
        (a.favorite.cmp(&b.favorite), Favorite),
    ];
    for (ord, crit) in steps {
        if ord != Ordering::Equal {
            return (ord, crit);
        }
    }
    let a_path = a.path.as_os_str().len();
    let b_path = b.path.as_os_str().len();
    let ord = b
        .mtime_ns
        .cmp(&a.mtime_ns)
        .then(b_path.cmp(&a_path))
        .then_with(|| b.path.cmp(&a.path));
    (ord, Tiebreak)
}

/// Returns the index of the recommended file and a plain-language reason.
/// Panics on an empty slice; groups always have at least two members.
pub fn recommend(files: &[FileRecord]) -> (usize, String) {
    let best = (0..files.len())
        .max_by(|&i, &j| compare(&files[i], &files[j]).0)
        .expect("group has members");

    // The criterion that separated the winner from its closest *different* rival explains
    // the choice. Byte-identical twins tie on everything, so they can't explain anything.
    let runner_up = (0..files.len())
        .filter(|&i| files[i].sha256 != files[best].sha256)
        .max_by(|&i, &j| compare(&files[i], &files[j]).0);
    let crit = match runner_up {
        Some(r) => compare(&files[best], &files[r]).1,
        None => Criterion::Tiebreak,
    };
    (best, reason(&files[best], crit, files))
}

fn reason(f: &FileRecord, crit: Criterion, all: &[FileRecord]) -> String {
    match crit {
        Criterion::Resolution | Criterion::Dimensions => format!(
            "Highest resolution ({}\u{00d7}{})",
            f.width.unwrap_or(0),
            f.height.unwrap_or(0)
        ),
        Criterion::Quality => "Sharpest image".into(),
        Criterion::Metadata => format!("Most complete photo details ({} EXIF tags)", f.exif_tags),
        Criterion::CreationDate => format!(
            "Earliest original ({})",
            f.taken_at.as_deref().unwrap_or("unknown date")
        ),
        Criterion::Favorite => "Marked as a favourite".into(),
        Criterion::Tiebreak => {
            if all.iter().all(|x| x.sha256 == f.sha256) {
                "All copies are identical; keeping the oldest file".into()
            } else {
                "Copies are equally good; keeping the oldest file".into()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn rec(id: i64) -> FileRecord {
        FileRecord {
            id,
            path: PathBuf::from(format!("/p/{id}.jpg")),
            size: 1000,
            mtime_ns: 1_000 + id,
            sha256: format!("sha-{id}"),
            ahash: Some(0),
            dhash: Some(0),
            phash: Some(0),
            width: Some(100),
            height: Some(100),
            sharpness: Some(500.0),
            exif_tags: 10,
            taken_at: None,
            favorite: false,
        }
    }

    #[test]
    fn higher_resolution_wins() {
        let mut big = rec(2);
        big.width = Some(4000);
        big.height = Some(3000);
        let (i, why) = recommend(&[rec(1), big, rec(3)]);
        assert_eq!(i, 1);
        assert!(why.contains("4000"), "{why}");
    }

    #[test]
    fn resolution_beats_metadata() {
        let mut rich = rec(1);
        rich.exif_tags = 80;
        let mut big = rec(2);
        big.width = Some(200);
        assert_eq!(recommend(&[rich, big]).0, 1);
    }

    #[test]
    fn small_sharpness_differences_do_not_count() {
        let mut slightly_sharper = rec(1);
        slightly_sharper.sharpness = Some(510.0);
        let mut richer = rec(2);
        richer.exif_tags = 30;
        let (i, why) = recommend(&[slightly_sharper, richer]);
        assert_eq!(i, 1, "{why}");
    }

    #[test]
    fn clearly_sharper_wins_over_metadata() {
        let mut sharp = rec(1);
        sharp.sharpness = Some(2000.0);
        let mut richer = rec(2);
        richer.exif_tags = 30;
        assert_eq!(recommend(&[richer, sharp]).0, 1);
    }

    #[test]
    fn earliest_known_date_wins() {
        let mut later = rec(1);
        later.taken_at = Some("2021-01-01 00:00:00".into());
        let mut earlier = rec(2);
        earlier.taken_at = Some("2019-06-01 12:00:00".into());
        let unknown = rec(3);
        let (i, why) = recommend(&[later, earlier, unknown]);
        assert_eq!(i, 1);
        assert!(why.contains("2019"));
    }

    #[test]
    fn favourite_breaks_a_tie() {
        let mut fav = rec(2);
        fav.favorite = true;
        assert_eq!(recommend(&[rec(1), fav]).0, 1);
    }

    #[test]
    fn identical_copies_keep_the_oldest_file() {
        let same = |id| FileRecord {
            sha256: "same".into(),
            ..rec(id)
        };
        let (i, why) = recommend(&[same(5), same(3), same(4)]);
        assert_eq!(i, 1, "rec(3) has the smallest mtime");
        assert!(why.contains("identical"));
    }

    #[test]
    fn reason_ignores_identical_twins_of_the_winner() {
        let mut big = rec(1);
        big.width = Some(4000);
        let twin = FileRecord {
            id: 2,
            path: "/p/2.jpg".into(),
            ..big.clone()
        };
        let (_, why) = recommend(&[big, twin, rec(3)]);
        assert!(why.contains("4000"), "{why}");
    }
}
