# Development guide

## Project management

There is no uv/npm-style umbrella tool. Each half uses its language's standard manager:

| | Rust engine | Flutter app |
|---|---|---|
| Manifest | `Cargo.toml` (workspace root + `core/`) | `app/pubspec.yaml` |
| Lockfile (committed) | `Cargo.lock` | `app/pubspec.lock` (after first `flutter pub get`) |
| Build / test | `cargo build` / `cargo test` | `flutter run` / `flutter test` |
| Toolchain pin | `rust-toolchain.toml` | Flutter SDK version |

## Windows setup WITHOUT the Microsoft C++ Build Tools

The normal Windows Rust setup (`x86_64-pc-windows-msvc`) needs Microsoft's C++ Build
Tools, which need admin rights to install. On a machine without them we use:

- **Rust, GNU variant** (`x86_64-pc-windows-gnu`): ships its own linker, needs no admin.
- **Zig** (`winget install zig.zig`): a portable C compiler. Needed because SQLite is C code.
- **cargo-zigbuild**: makes Zig do the C compiling, the Windows import libraries *and*
  the linking (via LLVM's LLD linker). Install the prebuilt binary:
  `python -m pip install --no-deps --target %TEMP%\czb cargo-zigbuild`, then copy
  `bin\cargo-zigbuild.exe` into `%USERPROFILE%\.cargo\bin`.

Why not simpler: the GNU toolchain's bundled `gcc` can only link, not compile C, and its
bundled `dlltool` needs an assembler that isn't shipped. A shim that swapped in Zig's
dlltool produced binaries that crashed at startup, because GNU `ld` mis-links LLVM-format
import libraries (a known issue noted in rustc's source). cargo-zigbuild avoids GNU `ld`
entirely.

```bash
source scripts/dev-env.sh     # PATH + CARGO_BUILD_TARGET=x86_64-pc-windows-gnu
cargo-zigbuild test           # instead of: cargo test
cargo-zigbuild clippy --all-targets
cargo fmt
```

Do **not** compile cargo-zigbuild itself with `cargo install` on such a machine: it gets
linked by the broken path and crashes.

Machines with the MSVC Build Tools (and macOS/Linux) use plain `cargo test`.

## Getting the Flutter app running (not done yet)

Flutter on Windows desktop **also requires the MSVC Build Tools** ("Desktop development
with C++"), so the admin-rights problem has to be solved for the Windows app regardless.

1. Install Flutter ≥ 3.35 and the MSVC Build Tools (or build on another machine).
2. `cargo install flutter_rust_bridge_codegen --version 2.13.0`
3. In `app/`: `flutter create --platforms=windows,android,macos,linux .`
   (generates the platform folders and keeps existing `lib/`, `pubspec.yaml`).
4. Wire the Rust crate into the Flutter build: run `flutter_rust_bridge_codegen integrate`
   in a scratch project and copy its `rust_builder/` folder and pubspec entry. **Don't run it
   in `app/`**, because it may overwrite `app/rust/`.
5. `flutter_rust_bridge_codegen generate` creates `app/rust/src/frb_generated.rs` and `app/lib/src/rust/`.
6. Add `"app/rust"` to the workspace `members` in the root `Cargo.toml` (remove it from `exclude`).
7. `flutter test` (selection model tests), then `flutter run`.

The UI can be tried before steps 2–6: `flutter run -t lib/main_demo.dart` uses made-up
data (`lib/src/demo_service.dart`) instead of the engine.

Expect a round of compile fixes the first time: the Dart code was written against
flutter_rust_bridge's naming conventions without the generator available to check it
(`lib/src/rust_service.dart` is the only file that touches generated code).

## Safety rules (enforced in `core/src/lib.rs`, tested in `core/tests/end_to_end.rs`)

- Only folders the user selected are walked; symlinks are not followed out of them.
- Folders are stored canonicalised, so one folder added under two spellings (`C:\Photos` /
  `c:\photos`, a junction, a `subst` drive) is stored once. Otherwise every photo would look like a
  duplicate of itself, and deleting the "copy" would delete the only file.
- Files are opened read-only during scanning.
- `Engine::delete` refuses the **whole** request if any id is unknown or if it would
  remove every copy in a group.
- A file is skipped (not deleted) if its size or modification time changed since the scan,
  or if it is no longer inside a selected folder.
- Trash/Recycle Bin is the default; permanent deletion must be chosen explicitly.

## Interpretations of the PRD (please confirm)

| PRD item | What was built |
|---|---|
| "Favorite flag (if available)" | EXIF/XMP star rating ≥ 4 counts as favourite. |
| "Better image quality" | Sharpness (edge strength on a normalised thumbnail); differences under ~19% are treated as equal. |
| "Largest dimensions" vs "Highest resolution" | Resolution = pixel count; dimensions = longest side (tie-breaker). |
| Batch actions | *Keep Recommended* and *Clear Selection* apply to all groups; *Restore Group* resets one group to the recommendation; *Manual Selection* = tap a photo; *Review Before Deleting* = the review screen. |
| Favourite is listed last | Kept in PRD order, so it only breaks exact ties. Worth reconsidering: users may expect a favourite to always be kept. |

## Known gaps

- **HEIC/HEIF/AVIF** (iPhone photos): found and matched as exact duplicates only; no
  pure-Rust decoder for perceptual hashing.
- **Android**: Rust reads files by path, which on Android 11+ needs "All files access"
  (`MANAGE_EXTERNAL_STORAGE`), and Google Play restricts that permission. The alternative,
  MediaStore/SAF, means reading files through Dart/Kotlin. No OS trash on Android:
  permanent delete only (Android's MediaStore trash would need a platform channel).
- Thresholds: pHash ≤ 8 and dHash ≤ 12 were tuned on synthetic images. The pixel check
  (≤ 18) was calibrated on real screenshots (distinct screens scored 35–71) against copies
  degraded down to 25% size at JPEG quality 30 (≤ 10). It hasn't been tuned on a large camera
  library yet. Bump `LOCAL_DIFFERENCE_METHOD` in `core/src/lib.rs` whenever the
  scoring algorithm changes, so cached scores are recomputed.
- The pixel check also rejects **crops and rotations by an editor** (not EXIF rotation). Those
  belong to Phase 3 ("similar"), not duplicates.
- No scan cancellation yet.
