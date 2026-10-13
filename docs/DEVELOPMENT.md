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

## Building the Tauri app (`desktop/`) without MSVC

```bash
source scripts/desktop-env.sh   # once per shell; builds two small helper tools on first use
cd desktop
cargo build                     # plain cargo, NOT cargo-zigbuild
DUPELENS_DB=/tmp/test.db ./target/debug/dupelens-desktop.exe   # optional: a throwaway database
```

`cargo-zigbuild build` works for the CLI but **not** for Tauri. Three things had to be fixed,
all in the programs Cargo builds and runs *during* the build (build scripts and proc-macros),
which a plain CLI barely has:

| Symptom | Cause | Fix in `scripts/desktop-env.sh` |
|---|---|---|
| Tauri's build script crashes on start: `0xc0000005` (access violation), before printing anything | With a target set (`cargo-zigbuild` always sets one), Cargo applies the linker setting **only to the app**. Build scripts are linked by Rust's bundled GNU `ld`, which produces broken programs here (see [0002](decisions/0002-windows-build-without-admin.md)). Exactly which input `ld` mishandles wasn't pinned down: the crashing and a working build script have identical import tables. | No target: `CARGO_BUILD_TARGET` is unset, so the linker setting covers build scripts and proc-macros too |
| `tauri-macros`: `lld-link: error: too many exported symbols (got 69808, max 65535)` | rustc passes a DLL's export list as `-Wl,…\list.def`. `cargo-zigbuild` 0.23 **skips** that argument on purpose (`src/zig/linker_args.rs`), so LLD exports every symbol. Small DLLs survive; big proc-macros don't. | `scripts/zig-cc.py` rewrites it to a plain `list.def` input, which LLD accepts, then hands everything else to `cargo-zigbuild zig cc` for its usual clean-up. It must keep rustc's response-file name `linker-arguments`, because `cargo-zigbuild` only cleans up files with that name. |
| `error calling dlltool 'dlltool.exe': program not found` | Import libraries for `windows-link` (raw-dylib) are made with `dlltool` | A copy of `cargo-zigbuild.exe` named `dlltool.exe` (it acts as Zig's dlltool under that name), in `target/zig-tools/` |
| `tauri-winres … NotAttempted("windres")` | The icon/version resource is compiled by `windres`, which isn't installed. `embed-resource` ignores `RC` on Windows-GNU. | `scripts/windres-zig.rs`: a tiny `windres.exe` that translates the arguments for `zig rc /:output-format coff` |

Also set: `CARGO_ZIGBUILD_ZIG_VERSION`/`CARGO_ZIGBUILD_ZIG_COMMAND` (what `cargo-zigbuild`'s own
wrappers set), and `CARGO_PROFILE_DEV_DEBUG=0` (with debug info, each Tauri build script is
~110 MB, and a debug build needs several GB of disk).

How it was checked: the app was launched with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`,
which lets the DevTools protocol drive the window. The `ui/` pages loaded, `invoke('api')` reached
`Service`, a scan ran, and all previews loaded through the `thumb` scheme.

On machines with the MSVC Build Tools (and on macOS/Linux), none of this is needed:
`cd desktop && cargo build`.

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
| Batch actions | *Keep Recommended* and *Clear Selection* apply to all groups (in the UI: to the groups shown by the current filter, labelled "(shown groups)"); *Restore Group* resets one group to the recommendation; *Manual Selection* = tap a photo; *Review Before Deleting* = the review screen. |
| Favourite is listed last | Kept in PRD order, so it only breaks exact ties. Worth reconsidering: users may expect a favourite to always be kept. |

## Known gaps

- **HEIC/HEIF/AVIF** (iPhone photos): found and matched as exact duplicates only; no
  pure-Rust decoder for perceptual hashing.
- **Android**: Rust reads files by path, which on Android 11+ needs "All files access"
  (`MANAGE_EXTERNAL_STORAGE`), and Google Play restricts that permission. The alternative,
  MediaStore/SAF, means reading files through Dart/Kotlin. No OS trash on Android:
  permanent delete only (Android's MediaStore trash would need a platform channel).
- Thresholds: pHash ≤ 8 and dHash ≤ 12 were tuned on synthetic images. The pixel check (≤ 18)
  was validated on a test library built from 112 real screenshots, with 38 planted copies
  (re-saved JPEG q60, WebP q70, WhatsApp-style 1600 px q70, half size q75, quarter size q40):
  38/38 found, 0 false groups. Genuine copies scored ≤ 8.9, and distinct screenshots ≥ 34.1.
  It hasn't been tested on real camera photos or burst shots yet. Bump
  `LOCAL_DIFFERENCE_METHOD` in `core/src/lib.rs` whenever the scoring changes, so cached
  scores are recomputed.
- The pixel check also rejects **crops and rotations by an editor** (not EXIF rotation). Those
  belong to Phase 3 ("similar"), not duplicates.
- No scan cancellation yet.

## Performance notes

Benchmark: 160 synthetic 12-megapixel JPEGs (~2 MB each), 12-core laptop, release build.

| Stage | Result |
|---|---|
| First scan, including pixel checks | ~3.6 s |
| Rescan, nothing changed | ~0.1 s |
| Full JPEG decode + 512 px thumbnail (`image` crate) | ~76 ms per photo, the bulk of analysis time |

Things tried and rejected, so nobody repeats them:

- **Reduced-scale JPEG decoding** (`jpeg-decoder` at 1/8 size): *slower* (~88 ms per photo),
  because most of the cost is the compressed-data (entropy) decoding, which can't be skipped,
  and the `image` crate's decoder is much better optimised.
- **Quick `thumbnail()` pre-shrink before the pixel check:** fast, but its aliasing pushed
  genuine copies' scores up to 16.8, too close to the threshold of 18. The pixel check now
  converts to grayscale and shrinks with a smooth (Triangle) filter: same speed, max 8.9.

The pixel check runs on all CPU cores, only for pairs that already passed the hash checks,
and each pair's score is cached in SQLite.
