# 0004: Pixel-level check to reject look-alike false positives

- **Status:** Accepted
- **Date:** 2026-10-06

## Context
On the maintainer's real `Pictures` folder, screenshots of the *same browser window taken
minutes apart* (only a timer, a few ticks and the clock differed) were grouped as
duplicates. Their pHashes were **identical** (distance 0): perceptual hashes only see the
broad layout of a 32×32 thumbnail, so small changes vanish. dHash and aspect ratio passed
too.

## Decision
Every hash-matched pair must also pass a **local difference** check
(`hashing::local_difference`):
1. Shrink both images to a common grid: **the smaller image's own size, capped at 512**.
2. Blur lightly (cancels compression noise), split into **8×8-pixel patches**.
3. Score = mean difference of the **most different patch**. Above **18** means not a duplicate.

Why it works: a re-encoded or resized copy differs a little *everywhere*, while a
different screenshot is identical almost everywhere but differs *a lot in a few spots*.

Scores are cached in SQLite (`local_differences` table) per pair of contents, with a
**method version** (`LOCAL_DIFFERENCE_METHOD` in `core/src/lib.rs`). Bump it whenever the
scoring changes, so stale scores are recomputed.

## Calibration (keep updated)
Test library: 112 real screenshots + 38 planted copies (re-saved JPEG q60, WebP q70,
WhatsApp-style 1600 px q70, half size q75, quarter size q40).
- Genuine copies: score ≤ **8.9**. Different screenshots: ≥ **34.1**. Threshold 18 sits in between.
- 38/38 found, 0 false groups, original recommended every time.

## History / rejected variants
- **Always compare at 512×512** (first version): enlarging small copies made their blurred
  text clash with the crisp original, so genuine half/quarter copies scored up to **117**
  and 8 of 38 were missed. Fixed by comparing at the smaller image's resolution.
- **Rough `thumbnail()` pre-shrink** (for speed): aliasing pushed genuine copies to 16.8,
  too close to 18. Replaced by grayscale + smooth (Triangle) shrink; see 0009.

## Consequences
- Pixel checks read the two files again (only for hash-matched pairs, cached afterwards).
- Not yet validated on real camera photos or burst shots. Burst shots are expected to be
  rejected (different moments), which matches the PRD (Phase 3 "similar").
