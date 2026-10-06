# 0001: Rust core engine; Flutter UI as specified by the PRD

- **Status:** Accepted. The UI part is on hold in favour of [0008](0008-ui-local-web-first.md).
- **Date:** 2026-10-05

## Context
The PRD ([PRD-v1](../PRD-v1.md)) specifies a Flutter UI on top of a Rust core, targeting
Android, Windows, macOS, Linux (iOS later). Neither toolchain was installed on the
development laptop, and installing Flutter for Windows needs admin rights (see 0002).

## Decision
- All logic lives in a Rust library, `core/` (`dupelens-core`): scanning, hashing,
  grouping, recommendations, SQLite storage and deletion. Any UI is a thin layer over it.
- A command-line tool (`dupelens`) ships with the core, so the engine is usable and
  testable without any UI.
- The Flutter UI (`app/lib`) and its flutter_rust_bridge layer (`app/rust`) were written
  but **never compiled**, because Flutter isn't installed. The UI talks to a small
  `DupeLensService` interface, with a demo implementation, so the UI can run before the
  Rust bridge is generated.

## Why
Keeping every rule in the Rust core (especially the deletion safety rules, 0006) means
each UI (CLI, browser, Flutter, Tauri) gets the same behaviour without re-implementing it.

## Consequences
- Switching UI technology is cheap, and in fact happened (0008).
- `app/` contains unverified code; expect compile fixes if Flutter is ever resumed.
  Steps are in [DEVELOPMENT.md](../DEVELOPMENT.md#getting-the-flutter-app-running-not-done-yet).
