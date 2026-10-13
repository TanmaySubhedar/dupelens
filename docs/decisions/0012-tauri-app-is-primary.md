# 0012: The Tauri app is the main program; the browser UI becomes a fallback

- **Status:** Accepted (supersedes [0008](0008-ui-local-web-first.md); the browser host it kept as a fallback was removed 2026-10-10, see Decision)
- **Date:** 2026-10-10

## Context
0008 chose a browser UI for v0.1 because Tauri seemed unbuildable on the no-admin Windows
machine. Since then the Tauri app builds and runs there ([0011](0011-one-ui-two-hosts.md),
DEVELOPMENT.md). The maintainer's goals for the app: Rust-native, one application to download,
a CLI-like experience, and a path to Android. All of these point to Tauri.

## Decision
- **`dupelens` is the Tauri app** (`desktop/`). Started without a command (double-clicked), it
  opens the window. `dupelens <command>` in a terminal runs the CLI commands (`folders`,
  `scan`, `groups`, `clean`).
- The CLI commands moved from the old binary into `dupelens_core::cli` (`cli` feature), so
  both programs share one implementation.
- On Windows the app is a **console program**, so terminals wait for it, show its output and
  let `clean` ask for confirmation. When double-clicked, it closes the console Windows opened
  for it (`hide_own_console`), which may show as a brief flash.
- The old binary is renamed **`dupelens-cli`**: the same commands without a window, for
  servers and minimal Linux, where the app can't start (it links against WebKitGTK).
- **The browser host was removed on 2026-10-10.** `dupelens-cli ui`, `core/src/web.rs` and the
  browser branches of `ui/js/api.js` are gone. Reasons: WebKitGTK is present on practically
  all desktop Linux, and packaging (.deb/.rpm dependencies, AppImage/Flatpak bundling) handles
  the rest; machines without it are mostly headless servers, where a localhost-only browser UI
  didn't help either; it was a second attack surface (a local server that can delete files)
  and double testing; and `api.js` keeps re-adding cheap if a browser mode is ever needed.

## Consequences
- Users get one program for both the window and the terminal.
- There is no local web server or token any more: the app is the only UI host.
- Windows builds made without MSVC need `WebView2Loader.dll` beside `dupelens.exe`; MSVC
  builds (planned for CI releases) link it in, giving a single `.exe`.
