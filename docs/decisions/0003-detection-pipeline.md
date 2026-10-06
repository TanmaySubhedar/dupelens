# 0003: Detection pipeline (SHA-256, perceptual hashes, BK-tree)

- **Status:** Accepted
- **Date:** 2026-10-05 (pixel check added by [0004](0004-pixel-level-check.md))

## Context
The PRD asks for exact duplicates (SHA-256) and near duplicates (pHash/dHash/aHash), with
"metadata verification", grouping and a recommendation.

## Decision
1. **Exact:** files with the same SHA-256 are byte-identical duplicates.
2. **Near:** perceptual hashes are 64-bit fingerprints of what a picture looks like. One
   file stands in for each distinct SHA-256; candidates must have pHash ≤ 8 bits apart.
   A **BK-tree** finds them without comparing every pair.
3. **Verification:** candidates must also agree on dHash (≤ 12) and aspect ratio (within
   5%), then pass the pixel check (0004).
4. **Grouping:** union-find merges matches, so an exact copy of a near duplicate joins the
   same group.
5. EXIF orientation is applied before hashing, so a rotated-by-flag photo matches a copy
   whose pixels were rotated.
6. Formats: JPEG, PNG, WebP, GIF, BMP, TIFF are decoded. HEIC/HEIF/AVIF get exact
   matching only (no pure-Rust decoder).
7. Results are cached in SQLite, keyed by path, size and modification time, so rescans
   only analyse new or changed files.

## Consequences
- Rotations or crops done in an editor are *not* duplicates. The PRD puts those in
  Phase 3 ("similar"), not duplicates.
- iPhone HEIC photos only match when byte-identical (a known gap).
