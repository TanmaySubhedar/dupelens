//! Turns analysed files into duplicate groups.
//!
//! Pipeline (PRD "Detection Pipeline"):
//! 1. Files with the same SHA-256 are confirmed duplicates.
//! 2. One representative per distinct SHA-256 is matched against the others by pHash,
//!    using a BK-tree so we never compare every pair.
//! 3. Verification: a pHash match must also agree on dHash and aspect ratio, and then pass a
//!    pixel-level comparison. Hashes only see the broad layout, so two screenshots of the same
//!    window with different text hash identically; the pixel check catches the changed spots.
//! 4. Matches are merged with union-find into groups.
//! 5. Each group gets a recommendation.

use crate::hashing::hamming;
use crate::model::{DuplicateGroup, FileRecord, GroupKind, GroupMember};
use crate::recommend::recommend;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct GroupingConfig {
    /// Max pHash distance (of 64 bits) for two images to count as near duplicates.
    pub phash_max: u32,
    /// Max dHash distance, checked as a second opinion to cut false positives.
    pub dhash_max: u32,
    /// Max relative difference in aspect ratio (0.05 = 5%).
    pub aspect_tolerance: f64,
    /// Max [`crate::hashing::local_difference`] score. Re-encoded or resized copies of one
    /// picture score under ~10; pictures with real changes, even small ones, score over ~35.
    pub max_local_difference: f64,
}

impl Default for GroupingConfig {
    fn default() -> Self {
        Self {
            phash_max: 8,
            dhash_max: 12,
            aspect_tolerance: 0.05,
            max_local_difference: 18.0,
        }
    }
}

struct UnionFind(Vec<usize>);

impl UnionFind {
    fn new(n: usize) -> Self {
        Self((0..n).collect())
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.0[x] != x {
            self.0[x] = self.0[self.0[x]];
            x = self.0[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0[ra.max(rb)] = ra.min(rb);
        }
    }
}

/// Burkhard-Keller tree over 64-bit hashes with Hamming distance. Children are keyed by
/// their distance to the parent; the triangle inequality lets a radius query skip whole
/// subtrees that cannot contain a match.
struct BkTree {
    nodes: Vec<(u64, usize, HashMap<u32, usize>)>, // (hash, item index, children)
}

impl BkTree {
    fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    fn insert(&mut self, hash: u64, item: usize) {
        if self.nodes.is_empty() {
            self.nodes.push((hash, item, HashMap::new()));
            return;
        }
        let mut cur = 0;
        loop {
            let d = hamming(hash, self.nodes[cur].0);
            match self.nodes[cur].2.get(&d) {
                Some(&next) => cur = next,
                None => {
                    let idx = self.nodes.len();
                    self.nodes.push((hash, item, HashMap::new()));
                    self.nodes[cur].2.insert(d, idx);
                    return;
                }
            }
        }
    }

    fn within(&self, hash: u64, radius: u32, out: &mut Vec<usize>) {
        if self.nodes.is_empty() {
            return;
        }
        let mut stack = vec![0];
        while let Some(i) = stack.pop() {
            let (h, item, children) = &self.nodes[i];
            let d = hamming(hash, *h);
            if d <= radius {
                out.push(*item);
            }
            for (&cd, &child) in children {
                if cd + radius >= d && cd <= d + radius {
                    stack.push(child);
                }
            }
        }
    }
}

fn aspect_close(a: &FileRecord, b: &FileRecord, tol: f64) -> bool {
    match (a.aspect_ratio(), b.aspect_ratio()) {
        (Some(x), Some(y)) => (x - y).abs() / x.max(y) <= tol,
        _ => false,
    }
}

fn is_near_match(a: &FileRecord, b: &FileRecord, cfg: &GroupingConfig) -> bool {
    let (Some(da), Some(db)) = (a.dhash, b.dhash) else {
        return false;
    };
    hamming(da, db) <= cfg.dhash_max && aspect_close(a, b, cfg.aspect_tolerance)
}

/// Pixel-level comparison of two files that passed the hash checks: returns their
/// [`crate::hashing::local_difference`] score, or `None` if they could not be compared.
pub type LocalDifferenceFn<'a> = dyn FnMut(&FileRecord, &FileRecord) -> Option<f64> + 'a;

/// Index of the first file with each distinct SHA-256.
fn first_of_each_content(files: &[FileRecord]) -> HashMap<&str, usize> {
    let mut by_sha: HashMap<&str, usize> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        by_sha.entry(f.sha256.as_str()).or_insert(i);
    }
    by_sha
}

/// Pairs of files with *different* contents whose pHash, dHash and aspect ratio match:
/// the candidates that still need the pixel check. One file stands in for each distinct
/// content, so byte-identical copies never need comparing.
pub fn near_candidates(files: &[FileRecord], cfg: &GroupingConfig) -> Vec<(usize, usize)> {
    let mut reps: Vec<usize> = first_of_each_content(files)
        .into_values()
        .filter(|&i| files[i].phash.is_some())
        .collect();
    reps.sort_unstable();
    let mut tree = BkTree::new();
    let mut hits = Vec::new();
    let mut pairs = Vec::new();
    for &i in &reps {
        let ph = files[i].phash.expect("filtered");
        hits.clear();
        tree.within(ph, cfg.phash_max, &mut hits);
        pairs.extend(
            hits.iter()
                .filter(|&&j| is_near_match(&files[i], &files[j], cfg))
                .map(|&j| (j, i)),
        );
        tree.insert(ph, i);
    }
    pairs
}

pub fn build_groups(
    files: Vec<FileRecord>,
    cfg: &GroupingConfig,
    local_difference: &mut LocalDifferenceFn,
) -> Vec<DuplicateGroup> {
    let mut uf = UnionFind::new(files.len());

    // 1. Exact duplicates.
    let firsts = first_of_each_content(&files);
    for (i, f) in files.iter().enumerate() {
        uf.union(firsts[f.sha256.as_str()], i);
    }

    // 2-3. Near duplicates: hash candidates that also pass the pixel check.
    for (i, j) in near_candidates(&files, cfg) {
        if local_difference(&files[i], &files[j]).is_some_and(|d| d <= cfg.max_local_difference) {
            uf.union(i, j);
        }
    }

    // 4. Collect groups of two or more.
    let mut clusters: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..files.len() {
        let root = uf.find(i);
        clusters.entry(root).or_default().push(i);
    }

    let mut groups: Vec<DuplicateGroup> = clusters
        .into_values()
        .filter(|c| c.len() > 1)
        .map(|c| make_group(c.iter().map(|&i| files[i].clone()).collect()))
        .collect();

    // Biggest wins first.
    groups.sort_by(|a, b| {
        b.reclaimable_bytes
            .cmp(&a.reclaimable_bytes)
            .then(a.id.cmp(&b.id))
    });
    groups
}

fn make_group(mut files: Vec<FileRecord>) -> DuplicateGroup {
    files.sort_by_key(|f| f.id);
    let (best, reason) = recommend(&files);
    let keep = files[best].clone();
    let kind = if files.iter().all(|f| f.sha256 == keep.sha256) {
        GroupKind::Exact
    } else {
        GroupKind::Near
    };
    let reclaimable_bytes = files
        .iter()
        .filter(|f| f.id != keep.id)
        .map(|f| f.size)
        .sum();
    let members = files
        .into_iter()
        .map(|f| GroupMember {
            identical_to_recommended: f.sha256 == keep.sha256,
            distance_to_recommended: match (f.phash, keep.phash) {
                (Some(a), Some(b)) => Some(hamming(a, b)),
                _ => None,
            },
            file: f,
        })
        .collect::<Vec<_>>();
    DuplicateGroup {
        id: members.iter().map(|m| m.file.id).min().expect("non-empty"),
        kind,
        recommended_id: keep.id,
        reason,
        reclaimable_bytes,
        members,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn rec(id: i64, sha: &str, phash: u64, dhash: u64, w: u32, h: u32) -> FileRecord {
        FileRecord {
            id,
            path: PathBuf::from(format!("/p/{id}.jpg")),
            size: 100 * id as u64,
            mtime_ns: id,
            sha256: sha.into(),
            ahash: Some(0),
            dhash: Some(dhash),
            phash: Some(phash),
            width: Some(w),
            height: Some(h),
            sharpness: Some(100.0),
            exif_tags: 0,
            taken_at: None,
            favorite: false,
        }
    }

    #[test]
    fn identical_bytes_form_an_exact_group() {
        let files = vec![
            rec(1, "aaa", 0, 0, 10, 10),
            rec(2, "aaa", 0, 0, 10, 10),
            rec(3, "bbb", u64::MAX, u64::MAX, 10, 10),
        ];
        let groups = build_groups(files, &GroupingConfig::default(), &mut |_, _| Some(0.0));
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, GroupKind::Exact);
        assert_eq!(groups[0].members.len(), 2);
    }

    #[test]
    fn similar_hashes_form_a_near_group() {
        let files = vec![
            rec(1, "a", 0b1111, 0b1111, 4000, 3000),
            rec(2, "b", 0b0111, 0b0011, 2000, 1500), // 1 and 2 bits off
            rec(3, "c", !0b1111, !0b1111, 4000, 3000), // far away
        ];
        let groups = build_groups(files, &GroupingConfig::default(), &mut |_, _| Some(0.0));
        assert_eq!(groups.len(), 1);
        let g = &groups[0];
        assert_eq!(g.kind, GroupKind::Near);
        assert_eq!(g.recommended_id, 1, "larger image is recommended");
        assert_eq!(g.default_deletion(), vec![2]);
    }

    #[test]
    fn aspect_ratio_mismatch_blocks_a_near_match() {
        let files = vec![rec(1, "a", 0, 0, 4000, 3000), rec(2, "b", 0, 0, 3000, 3000)];
        assert!(build_groups(files, &GroupingConfig::default(), &mut |_, _| Some(0.0)).is_empty());
    }

    #[test]
    fn exact_copies_of_a_near_match_join_one_group() {
        let files = vec![
            rec(1, "a", 0, 0, 100, 100),
            rec(2, "a", 0, 0, 100, 100),
            rec(3, "b", 1, 1, 50, 50),
        ];
        let groups = build_groups(files, &GroupingConfig::default(), &mut |_, _| Some(0.0));
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members.len(), 3);
    }

    #[test]
    fn pixel_check_can_veto_a_hash_match() {
        let files = vec![rec(1, "a", 0, 0, 100, 100), rec(2, "b", 0, 0, 100, 100)];
        let cfg = GroupingConfig::default();
        assert!(build_groups(files.clone(), &cfg, &mut |_, _| Some(40.0)).is_empty());
        assert!(build_groups(files.clone(), &cfg, &mut |_, _| None).is_empty());
        assert_eq!(build_groups(files, &cfg, &mut |_, _| Some(5.0)).len(), 1);
    }

    #[test]
    fn exact_duplicates_never_need_the_pixel_check() {
        let files = vec![rec(1, "a", 0, 0, 100, 100), rec(2, "a", 0, 0, 100, 100)];
        let groups = build_groups(files, &GroupingConfig::default(), &mut |_, _| {
            panic!("identical bytes must not be pixel-compared")
        });
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn bk_tree_finds_exactly_the_neighbours_in_radius() {
        let hashes: Vec<u64> = (0..500u64)
            .map(|i| i.wrapping_mul(0x9E37_79B9_7F4A_7C15))
            .collect();
        let mut tree = BkTree::new();
        for (i, &h) in hashes.iter().enumerate() {
            tree.insert(h, i);
        }
        for &q in hashes.iter().step_by(37) {
            let mut got = Vec::new();
            tree.within(q, 20, &mut got);
            got.sort();
            let want: Vec<usize> = (0..hashes.len())
                .filter(|&i| hamming(q, hashes[i]) <= 20)
                .collect();
            assert_eq!(got, want);
        }
    }
}
