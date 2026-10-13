# 0011: One UI, two hosts: the browser UI is built to run unchanged in Tauri

- **Status:** Accepted. Note: the browser host was removed on 2026-10-10 ([0012](0012-tauri-app-is-primary.md)); the one-UI idea and `api.js` as the only transport file still hold, now with a single host.
- **Date:** 2026-10-07

## Context
[0008](0008-ui-local-web-first.md) chose a local browser UI for v0.1, with a Tauri desktop
app later. The maintainer asked that the browser UI be compatible with Tauri from the start,
so v0.3 only wraps it and doesn't rewrite it. The two hosts differ in practice:

| | Browser host | Tauri |
|---|---|---|
| Page origin | `http://127.0.0.1:<port>` | `tauri://localhost` / `http://tauri.localhost` |
| Calling Rust | HTTP requests | `invoke()` IPC (inter-process messages) |
| Images from Rust | an HTTP URL | a custom URI scheme |
| Folder dialog | must be opened by the server | Tauri's dialog plugin |
| Auth | needs a secret token | not needed (no network listener) |

## Decision
- **A transport-neutral service layer in the core**: `dupelens_core::service::Service`. It
  holds the engine, runs scans on a background thread, caches results and previews, and
  exposes `call(cmd, json_args) -> json` and `thumbnail(id, large)`. Hosts only move messages.
- **One command list**, used by both hosts. The browser host maps `POST /api/<cmd>`. The
  Tauri host registers one command, `api(cmd, args)`. Host-specific commands (`pick_folder`,
  `quit`) are implemented by each host. The contract is in [docs/UI.md](../UI.md).
- **One transport module in the page**, `ui/js/api.js`. It detects `window.__TAURI__` and uses
  `invoke`, or uses `fetch` plus the token otherwise. No other UI file knows which host it
  runs in.
- **Page rules that hold in both hosts**: relative paths; no inline scripts or styles (strict
  CSP); no `alert`/`confirm`/`prompt`; no build step; long work is started and then polled.
- The web server is behind the core's default `web` Cargo feature. A Tauri app depends on
  `dupelens-core` with `default-features = false`, so it doesn't compile an HTTP server.
- A minimal Tauri 2 host is kept in `desktop/`, outside the Cargo workspace, as the starting
  point for v0.3. It builds on the no-admin toolchain with `scripts/desktop-env.sh` (four Zig
  workarounds, explained in DEVELOPMENT.md). **Tested 2026-10-08** in the real window: the
  unchanged `ui/` loads, `api.js` picks the Tauri transport, a scan runs through `invoke`, and
  all previews load via the `thumb` scheme.

## Options considered
| Option | Verdict |
|---|---|
| **Generic `api(cmd, args)` command** | **Chosen.** The command list exists once (in `Service::call`), so both hosts can't drift apart. |
| One `#[tauri::command]` per operation | More idiomatic Tauri, but the list is duplicated in each host and in the HTTP router |
| Tauri page talks to the localhost HTTP server too | Keeps one transport, but needs a network listener and token inside a desktop app, which is the attack surface Tauri lets us drop |
| Server-sent events / WebSocket for progress | Polling every 300 ms is simple, behaves the same over IPC and HTTP, and is plenty for a progress bar |

## Consequences
- Adding a feature means adding a command to `Service::call` (and to docs/UI.md). Both
  hosts get it for free.
- Adding a file to `ui/` also means adding it to `ASSETS` in `core/src/web.rs`.
- The service keeps groups in memory after a scan. Very large libraries cost memory roughly
  proportional to the number of duplicate files, not the library size.
