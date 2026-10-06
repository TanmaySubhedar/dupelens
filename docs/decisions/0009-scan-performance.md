# 0009: "Not lazy": scan speed work and rejected optimisations

- **Status:** Accepted
- **Date:** 2026-10-06

## Context
A core promise (0007) is results the moment you ask, unlike Files by Google, which waits
on background indexing. That's only credible if the first scan of a real photo library is
fast.

## Measurements
Benchmark: 160 synthetic 12-megapixel JPEGs (~2 MB each, phone-camera size) with 10
planted duplicates, 12-core laptop, release build.

| Version | First scan | Rescan |
|---|---|---|
| Before optimisation | 6.1 s | ~0.1 s |
| **Current** | **3.6 s** | ~0.1 s |

Roughly: 10,000 photos ≈ 3–4 minutes the first time, seconds afterwards. All 10 planted
duplicates were found both times.

## Decision
- Photo analysis runs in parallel across CPU cores (rayon).
- Pixel checks (0004) were the bottleneck: one pair at a time, on full 12-megapixel images.
  Now: candidate pairs are listed first (`grouping::near_candidates`), and uncached pairs
  are scored **in parallel**. Each image is converted to grayscale and **smoothly
  shrunk** to the 512 px comparison size right after decoding.

## Rejected (measured, don't retry without new evidence)
| Idea | Result |
|---|---|
| Reduced-scale JPEG decoding (`jpeg-decoder` at 1/8) | **Slower**: 88 ms vs 76 ms per photo. Most of the cost is entropy decoding, which can't be skipped, and the `image` crate's decoder is better optimised. |
| Rough `thumbnail()` pre-shrink to 512 | Fast, but genuine copies' scores rose to 16.8 (threshold 18). Unsafe. |
| Rough `thumbnail()` to 1024, then smooth | Max 11.2. Acceptable but worse than the final approach at the same speed. |

## Future ideas
- **Watch mode:** watch the selected folders and flag new duplicates as files arrive.
  The ultimate "not lazy".
- Show exact duplicates first while the look-alike analysis is still running.
