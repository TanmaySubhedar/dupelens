# The UI and the Tauri app

DupeLens has **one** user interface: the plain HTML/CSS/JavaScript in [`ui/`](../ui). It is shown
by the Tauri app ([`desktop/`](../desktop), the `dupelens` program), which connects the pages to
the engine ([0012](decisions/0012-tauri-app-is-primary.md)). This page is the contract between
the two.

The host is thin. All logic lives in `dupelens_core::service::Service`
([`core/src/service.rs`](../core/src/service.rs)), which wraps the engine, so the deletion
safety rules ([0006](decisions/0006-deletion-safety.md)) are enforced in Rust, not in the page.

```
ui/js/app.js ──► ui/js/api.js ──► Tauri invoke ──► desktop/src/main.rs ──► Service ──► Engine
  (screens)      (transport)                       (previews: thumb://)
```

There is no browser or server mode: opened in a plain browser, the page only shows
"DupeLens must be opened as the app."

## Commands

The page sends a command name and a JSON object of arguments, and gets JSON back. Errors come
back as `{ kind, message }`. `kind` is `bad_request` (a UI bug), `not_found`, `busy` (a scan
or comparison is running) or `failed` (the engine refused, e.g. an invalid folder or a
refused deletion). `message` is plain language, ready to show.

| Command | Arguments | Result |
|---|---|---|
| `info` | – | `{ version, trash_supported, platform }` |
| `folders` | – | `["C:\\Photos", ...]` |
| `add_folder` | `{ path }` | the stored (canonical) path |
| `remove_folder` | `{ path }` | `true` if it was selected |
| `start_scan` | – | `null`; the scan runs in the background |
| `scan_status` | – | `{ state: "idle" }`, `{ state: "running", phase, done, total }`, `{ state: "done", summary }` or `{ state: "failed", error }` |
| `groups` | – | duplicate groups from the last scan (cached) |
| `delete` | `{ ids, permanent }` | `{ deleted, failed: [{ id, path, reason }], bytes_freed }` |
| `pick_folder` * | – | a path, or `null` if cancelled or no dialog is available |
| `quit` * | – | `null`, then the app closes |

\* **Host commands.** The app implements these itself in `desktop/src/main.rs`, because they
need its own windowing (`quit` closes the window). Everything else is forwarded to
`Service::call`. `phase` is one of `discovering`,
`analyzing` (with `done`/`total`), `saving` or `comparing`.

Previews: JPEG, longest side 360 px (`size=small`) or 1600 px (`size=large`). Only files in
the current results can be previewed, so the page can never read an arbitrary path. Formats
that can't be decoded (HEIC) return 404 and the page shows a placeholder.

## Rules for `ui/`

1. **All backend access goes through `ui/js/api.js`.** No other file may call `invoke`, build
   preview URLs or touch `window.__TAURI__`.
2. **Relative paths only** (`style.css`, `js/app.js`).
3. **No inline scripts, inline styles or `on…=` attributes.** The app applies a strict
   Content-Security-Policy. Setting `element.style` from JavaScript is fine.
4. **No `alert` / `confirm` / `prompt`.** Not every webview supports them. Use the in-page
   `modal()` in `app.js`.
5. **No build step and no npm dependencies.** Tauri embeds `ui/` at compile time, so
   **after editing `ui/` rebuild `desktop/`** to see the change.
6. **Text goes in with `textContent`** (the `h()` helper in `dom.js`), never `innerHTML`, so a
   crafted file name can't inject markup.
7. Long-running work is started, then polled (`start_scan` + `scan_status`), and never held
   open in one call.

## Working on the UI

```bash
node --test ui/tests/*.test.js                 # selection rules (no npm install needed)
source scripts/desktop-env.sh                  # Windows without MSVC; see DEVELOPMENT.md
cd desktop && cargo build                      # re-embeds ui/; then run the app
```

## The Tauri app (`desktop/`)

`desktop/` is a minimal Tauri 2 app that loads `../ui` and forwards to `Service`.
It is outside the Cargo workspace on purpose: it has its own lockfile and `target/`, so the
core and `dupelens-cli` builds stay light.

**Builds and runs** (tested 2026-10-08): the full scan flow and previews work in the real window. On Windows without MSVC, use `source scripts/desktop-env.sh` then `cd desktop && cargo build`. Why plain `cargo-zigbuild` isn't enough is explained in [DEVELOPMENT.md](DEVELOPMENT.md#building-the-tauri-app-desktop-without-msvc). With MSVC, or on macOS/Linux: `cd desktop && cargo build`. Set `DUPELENS_DB` to use a throwaway database.

Requirements for it: `app.withGlobalTauri: true` (the page uses
`window.__TAURI__`), the `api` command, the `thumb` URI scheme, and a CSP that allows
`ipc:`/`http://ipc.localhost` for IPC and `thumb:`/`http://thumb.localhost` for images.
