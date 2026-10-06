# 0008: UI: local browser UI first (v0.1), Tauri later; Flutter code kept

- **Status:** Proposed (agreed direction; not built yet)
- **Date:** 2026-10-06

## Context
- Flutter for Windows needs the MSVC Build Tools (admin rights, unavailable; see 0002),
  and the Flutter code has never been compiled.
- The CLI alone can't meet the PRD's **gallery-first** principle: deciding what to delete
  is visual, and a terminal can't show the photos. It also can't serve Android users.
- For an open-source project, a Flutter + Rust + bridge stack is a heavy setup for
  contributors.

## Options considered
| Option | Verdict |
|---|---|
| A. HTML report (read-only) | Too limited: still delete via the CLI |
| **B. Local web UI** (`dupelens` serves a page on `127.0.0.1`, opens the browser) | **Chosen for v0.1**: no new toolchain, the whole PRD flow on desktop |
| C. Native Rust GUI (egui/Slint) | Discards the existing UI work; heavier |
| Flutter as in the PRD | On hold (blocked by MSVC; heavy for contributors) |

## Decision
- **v0.1:** build the UI as plain web technology (HTML/JS) served by the Rust binary.
- **Later:** wrap the *same* UI in **Tauri** (a native window, installers; Tauri 2 also
  targets Android/iOS), so no UI work is thrown away.
- **Keep the Flutter code** in the repo for now, in case the decision changes (maintainer's
  call). It stays unverified.

## Requirements this creates (must-haves for option B)
- Listen on **127.0.0.1 only**, on a random port, with a **secret token** in the opened
  URL. A page that can delete files must not be reachable by other websites or machines.
- Double-clicking the program starts the server, opens the browser, and offers a clear
  way to quit.
- No network access beyond localhost. Any update check must be opt-in.
