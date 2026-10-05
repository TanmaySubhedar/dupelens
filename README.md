# DupeLens

A privacy-first, offline duplicate photo finder. It scans only the folders you choose,
groups exact and look-alike copies, recommends the best copy to keep, and deletes nothing
without your explicit confirmation. Nothing is ever uploaded.

Built from `Duplicate_Photo_Finder_PRD_v1.md`. **Current scope: Phase 1 (images).**

## Status

| Part | State |
|---|---|
| Rust core engine (`core/`) | Built and tested: 24 unit + 11 end-to-end tests pass |
| `dupelens` CLI (`core/src/bin/`) | Built; tried by hand on a sample library |
| Flutter bridge (`app/rust/`) | Written, **not compiled**: needs flutter_rust_bridge codegen, which needs Flutter |
| Flutter UI (`app/lib/`) | Written, **not compiled or run**: Flutter is not installed on the dev machine yet |

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
   (within 5%), which cuts false positives.
4. **Grouping**: matches are merged, so an exact copy of a near duplicate lands in the same group.
5. **Recommendation**, by the PRD's criteria in order: resolution → dimensions → sharpness →
   EXIF richness → earliest capture date → favourite. Ties go to the oldest file.
