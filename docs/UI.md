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

This page has two halves: the **contract** between the pages and the app (below), and the
**visual design** the pages are being rebuilt to ([Visual design](#visual-design-v02-direction)).

## Commands

The page sends a command name and a JSON object of arguments, and gets JSON back. Errors come
back as `{ kind, message }`. `kind` is `bad_request` (a UI bug), `not_found`, `busy` (a scan
or comparison is running) or `failed` (the engine refused, e.g. an invalid folder or a
refused deletion). `message` is plain language, ready to show.

| Command | Arguments | Result |
|---|---|---|
| `info` | – | `{ version, trash_supported, undo_supported, platform }` |
| `folders` | – | `["C:\\Photos", ...]` |
| `add_folder` | `{ path }` | the stored (canonical) path |
| `remove_folder` | `{ path }` | `true` if it was selected |
| `start_scan` | – | `null`; the scan runs in the background |
| `start_regroup` | – | `null`; re-groups the last scan's results with the current matching setting in the background, without reading any folder. Poll `scan_status` |
| `cancel` | – | `true` if a scan or regroup was running and was asked to stop |
| `scan_status` | – | `{ state: "idle" }`, `{ state: "running", phase, done, total, bytes_done, bytes_total }`, `{ state: "done", summary }`, `{ state: "failed", error }` or `{ state: "cancelled" }` |
| `get_settings` | – | `{ preset, config, min_size, presets: [{ key, name, description, config }] }` |
| `set_settings` | `{ preset?, custom?, min_size? }` | the new settings. Drops the cached results: call `start_regroup` after a matching change; a changed `min_size` needs a new scan |
| `groups` | – | duplicate groups from the last scan (cached) |
| `delete` | `{ ids, permanent }` | `{ deleted, failed: [{ id, path, reason }], bytes_freed, batch }`. `batch` identifies this action in the history (`null` if nothing was deleted) |
| `stats` | – | `{ bytes_freed, files_deleted, last_cleaned, last_batch: { id, at, files, bytes, restorable } \| null }`: the lifetime total (not counting anything undone) and the last action |
| `undo` | `{ batch? }` | `{ restored, bytes, failed: [{ path, reason }] }`. Restores a Recycle Bin delete (the last one if `batch` is omitted). Refused with `failed` for a permanent delete, when there is nothing to undo, and on macOS. The page should rescan afterwards |
| `pick_folder` * | – | a path, or `null` if cancelled or no dialog is available |
| `suggested_folders` * | – | `[{ name, path }]`: the user's Pictures, Downloads, Desktop, Documents, Videos and Music folders that exist, for one-click choices on first run |
| `quit` * | – | `null`, then the app closes |

\* **Host commands.** The app implements these itself in `desktop/src/main.rs`, because they
need its own windowing (`quit` closes the window). Everything else is forwarded to
`Service::call`. `phase` is one of `discovering`,
`analyzing` (`done`/`total` files, and `bytes_done`/`bytes_total` so a single huge file still
moves the bar), `saving` or `comparing` (`done`/`total` look-alike pairs). A cancelled job
loses nothing: files already read are kept for the next scan.

Each group carries `category`, `kind` (`exact` or `near`) and `tentative` (found only because
matching is set looser than Balanced: the page must not pre-select it for deletion); each file
carries `category` and `modified_secs`. The scan summary carries `files_without_twin`,
`bytes_read`, `cloud_only_skipped` and `hard_links_skipped`.

Matching `preset` is one of `exact_only`, `strict`, `balanced`, `loose` or `custom`
([0014](decisions/0014-tunable-matching.md)). For `custom`, send
`custom: { phash_max, dhash_max, aspect_tolerance, max_local_difference }`; out-of-range values
are refused with `kind: "failed"`.

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

## Visual design (v0.2 direction)

Status: **agreed direction, not yet built.** The current pages in `ui/` still use the old look
(blue accent, rounded cards, emoji). The mock-ups this section was drawn from are not kept in
the repository; this page and the logo files are the reference.

### Principles

- **Clean, minimal, professional.** Quiet greys, one accent colour, thin lines instead of
  shadows, small corners (4 px). No emoji anywhere in the interface, and no decorative
  illustrations.
- **Data-forward.** Numbers (sizes, counts, dimensions) are set in a monospace face so they
  line up and read as measurements. Sentences are short and factual.
- **Thumbnails are always visible.** A person must never click to see what a group contains.
  Every copy in every group shows its thumbnail in the list, and pointing at a copy shows it
  large in a side pane. Keyboard shortcuts cover the rest (Space: full size; ← →: move between
  copies; K / D: keep or delete). Full size must not need an extra click.
- **Dark mode is first-class.** Every screen is designed and checked in light and dark. The theme
  follows the system setting (`prefers-color-scheme`).
- **Nothing is deleted without clear review.** The review screen shows each file to be
  removed next to the copy that stays, defaults to the Recycle Bin, and warns when
  non-photo files (videos, archives) are included.

### Colour

One accent (amber). Greys carry everything else. Keep/delete state is shown by amber (keep)
and red (delete), always with a text label as well as colour.

| Token | Light | Dark |
|---|---|---|
| page background | `#F6F5F2` | `#0F0F0E` |
| surface (panels, top bar) | `#FFFFFF` | `#171716` |
| text | `#151515` | `#EDEBE6` |
| muted text | `#5F5D57` | `#A3A09A` |
| border | `#DEDCD6` | `#2C2B28` |
| accent | `#B45309` | `#F2A33A` |
| accent soft (selected, notes) | `#FBEBD5` | `#2A2113` |
| text on accent | `#FFFFFF` | `#1A1100` |
| delete | `#B3261E` | `#F08A83` |
| delete soft | `#FBEAE8` | `#2E1C1B` |

Muted text on the page background passes 4.5:1 contrast in both themes; check any new colour
pair the same way.

### Type

IBM Plex Sans for text and IBM Plex Mono for numbers, sizes, paths and dimensions. Both must
be **bundled inside `ui/`** (the Content-Security-Policy forbids loading fonts from the
internet), with system fonts as the fallback.

### Layout and screens

A top bar with the logo and a plain text navigation (Overview, Results, Settings). No sidebar.

| Screen | What it holds |
|---|---|
| **Overview** | Reclaimable space as one large number, a stacked bar of reclaimable space by file type with a table under it, the folder list, and a side panel with the four matching presets and a **Re-run** button |
| **Scanning** | Percentage, four steps (finding, reading, comparing, saving), the file being read, plain-language counts (found, read, unchanged, cloud-only skipped, unreadable) and a **Found so far** panel that fills with the first duplicates while the scan runs, plus Cancel |
| **Results** | File-type filter chips with counts, the selection total and **Review and delete** in a toolbar. Left: groups, each with a row of thumbnails of all copies tagged Keep or Delete. Right (372 px): the large preview of the copy being pointed at, its facts, and Keep/Delete buttons |
| **Review and delete** | Every file to be removed beside the copy that stays, the total, Recycle Bin (default) or permanent delete, a notice when videos or archives are included, and the three safety promises |
| **Done** | Space freed, a breakdown by type, **Undo** and **Continue reviewing**, and a quiet panel with the lifetime total and the next milestone, stored only on this computer |
| **Settings** | Not yet designed. Holds the advanced matching values |

Group thumbnails: photos use the engine's thumbnails. For videos, PDFs and Office files, use the
operating system's thumbnail service where it exists (Windows); otherwise show a file-type tile
with the extension. A video thumbnail carries its duration.

The Recycle Bin does not release disk space until it is emptied. The Review and Done screens
say so in plain words.

### Matching control

"What counts as a duplicate" has four presets: **Exact only**, **Strict**, **Balanced** (today's
behaviour) and **Loose**. The Overview and Results screens show the current choice and let the
user change it; **Re-run** regroups from cached data without reading the folders again.
Matches that only the looser presets find are tagged "Maybe" and not pre-selected for
deletion. Advanced settings show the four raw values, each with a one-line explanation. How
these map onto `GroupingConfig` is still to be decided and recorded as a decision record.

### Logo

The mark is two outlined pills offset from each other, with the overlap filled amber: the same
thing twice. Files are in [`design/logo/`](design/logo/):

| File | Use |
|---|---|
| `dupelens-mark-light.svg` | On light backgrounds, 32 px and larger |
| `dupelens-mark-dark.svg` | On dark backgrounds, 32 px and larger |
| `dupelens-mark-small-light.svg` | 16 to 24 px on light backgrounds: heavier lines and a larger overlap |
| `dupelens-mark-small-dark.svg` | 16 to 24 px on dark backgrounds |

These are drawn but **not yet wired in**: `ui/icon.svg` and the app icons in `desktop/icons/`
still hold the old mark. Replacing them means regenerating `icon.ico` and the PNG sizes (and
an `.icns` for a future macOS build) from the small variant at the small sizes. Check the 16 px
render before shipping.

### Planned engine and command changes this design needs

These are not in the contract above yet. Each needs a decision record when built.

- ~~Exact matching for every file type~~ Done in the engine ([0013](decisions/0013-exact-matching-for-every-file-type.md)).
  `groups` results now carry `category` and `modified_secs` for each file, and the scan summary
  carries `files_without_twin`, `bytes_read` and `cloud_only_skipped`; the pages do not use
  them yet. Progress in bytes, scan cancellation and hard-link detection are done too.
- ~~Matching settings stored in the database, with commands to read and change them and to
  regroup from cached results (with progress and cancel)~~ Done
  ([0014](decisions/0014-tunable-matching.md)); the old pages have a basic preset menu, the
  redesign adds the full control and the Advanced values.
- A "not a duplicate" list.
- ~~Deletion history and undo for the Recycle Bin, and a local lifetime total~~ Done in the engine
  and commands ([0015](decisions/0015-deletion-history-and-undo.md)); the old pages have an Undo
  button and a "freed so far" line, and first-run folder suggestions. The milestone bar and the
  "found so far" panel during a scan are still to do with the redesign.
