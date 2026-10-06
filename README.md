# DupeLens

A privacy-first, offline duplicate photo finder for your desktop, in the spirit of the
duplicate cleaner in Files by Google: it recommends the best copy to keep, pre-selects the
rest, and cleans up when you confirm. Nothing is ever uploaded.

**Current scope: Phase 1 (images).**

## What it promises

- **You choose the folders.** DupeLens scans exactly the folders you add, and nothing else on
  your computer is ever read.
- **Results the moment you ask.** No background indexing to wait for: you press scan and get
  answers. On a 12-core laptop, a first scan of 160 phone-camera photos (12 megapixels each)
  takes about 3.6 seconds, and rescanning an unchanged library takes about 0.1 seconds,
  because files that didn't change are never re-read.
- **Finds look-alikes, not just exact copies.** Resized, recompressed, WhatsApp-style and
  converted copies are found too, while screenshots that only *look* similar (same window,
  different content) are kept apart.
- **Nothing is deleted without you.** Recycle Bin by default, at least one copy of every
  photo is always kept, and files changed since the scan are skipped.

## Status

| Part | State |
|---|---|
| Rust core engine (`core/`) | Built and tested: 26 unit + 13 end-to-end tests pass |
| `dupelens` CLI (`core/src/bin/`) | Built; tried by hand on a sample library |
| Flutter bridge (`app/rust/`) | Written, **not compiled**: needs flutter_rust_bridge codegen, which needs Flutter |
| Flutter UI (`app/lib/`) | Written, **not compiled or run**: Flutter is not installed on the dev machine yet |

## Project docs

- [docs/ROADMAP.md](docs/ROADMAP.md): where things stand, what's next, open questions
- [docs/decisions/](docs/decisions/README.md): why things are the way they are (one record per decision)
- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md): building, testing, toolchain setup
- [docs/PRD-v1.md](docs/PRD-v1.md): the original product requirements

## Layout

```
dupelens/
├── core/                 Rust engine (Cargo package `dupelens-core`)
│   ├── src/scanner.rs    walks ONLY the selected folders, finds image files
│   ├── src/metadata.rs   reads each file once: SHA-256, EXIF, dimensions, hashes
│   ├── src/hashing.rs    SHA-256 + aHash / dHash / pHash + sharpness
│   ├── src/grouping.rs   exact + near-duplicate grouping (BK-tree, union-find)
│   ├── src/recommend.rs  picks the copy to keep, with a plain-language reason
│   ├── src/db.rs         SQLite: folders + per-file cache for incremental rescans
│   ├── src/lib.rs        Engine API: folders, scan, groups, guarded delete
│   └── src/bin/dupelens.rs   command-line front-end
├── app/                  Flutter app
│   ├── rust/             bridge crate exposing the engine to Dart
│   └── lib/              UI (talks to the engine via lib/src/service.dart)
├── scripts/dev-env.sh    toolchain setup for Windows without MSVC (see docs)
└── docs/DEVELOPMENT.md   setup, design decisions, open questions
```

## Try the engine now (CLI)

```bash
source scripts/dev-env.sh            # Windows/Git Bash without MSVC; see docs/DEVELOPMENT.md
cargo-zigbuild build --release
B=target/x86_64-pc-windows-gnu/release/dupelens.exe

$B folders add "C:/Users/me/Pictures"
$B scan                 # first scan analyses everything; later scans only new/changed files
$B groups               # review what it found
$B clean                # keeps the recommended copy, moves the rest to the Recycle Bin after you type 'delete'
```

`clean` options: `--group <id>` (repeatable), `--exact-only`, `--permanent`, `--yes`.

## How detection works

1. **Exact duplicates**: files with the same SHA-256 (byte-for-byte identical).
2. **Near duplicates**: perceptual hashes are 64-bit fingerprints of what a picture looks
   like. A resized, recompressed or re-formatted copy gets almost the same fingerprint. Two
   photos match if their pHash differs by ≤ 8 of 64 bits.
3. **Verification**: a pHash match must also agree on dHash (≤ 12 bits) and aspect ratio
   (within 5%), and then pass a **pixel check**. Hashes only see the broad layout, so two
   screenshots of the same window with different text look identical to them. The pixel
   check shrinks both images to a common size (the smaller image's own size, at most 512), splits
   them into 8×8 patches and rejects the match if any patch differs strongly (score > 18). On a
   test library built from real screenshots, genuine copies (re-saved, WebP, WhatsApp-style,
   half and quarter size) scored ≤ 7.7, and different screenshots scored ≥ 34. Scores are
   cached in the database, so each pair is compared only once.
4. **Grouping**: matches are merged, so an exact copy of a near duplicate lands in the same group.
5. **Recommendation**, by the PRD's criteria in order: resolution → dimensions → sharpness →
   EXIF richness → earliest capture date → favourite. Ties go to the oldest file.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.
