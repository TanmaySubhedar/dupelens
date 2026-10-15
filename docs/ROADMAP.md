# Roadmap

Last updated: 2026-10-07. Decisions behind this plan are in [decisions/](decisions/README.md).

## Where things stand
| Part | State |
|---|---|
| Rust engine + CLI (Phase 1: images) | ✅ Built and tested on Windows; Linux build verified in a container |
| Flutter UI + bridge | ⏸ Written, never compiled; on hold ([0008](decisions/0008-ui-local-web-first.md)) |
| UI pages (`ui/`) | ✅ Full flow works (tested in Edge and in the Tauri window). ⬜ Visual redesign: clean, minimal, professional |
| `dupelens-cli` (window-less, same commands) | ✅ Kept for servers and minimal Linux. The browser host was removed 2026-10-10 ([0012](decisions/0012-tauri-app-is-primary.md)) |
| **The app `dupelens`** (`desktop/`, Tauri) | ✅ **Main program** ([0012](decisions/0012-tauri-app-is-primary.md)): window + CLI commands in one binary; builds without MSVC (`scripts/desktop-env.sh`) |
| Public repository / remote | ⬜ None yet |

## Milestones
1. **v0.1: desktop, usable by anyone**
   - ~~UI: folder picker, scan progress, photo grids with 👑, tap to toggle,
     review screen, Recycle Bin / permanent delete~~ ✅ done, now shown by the Tauri app
     ([0012](decisions/0012-tauri-app-is-primary.md), [docs/UI.md](UI.md)). Still to do: try it on
     a real photo library (thousands of photos), and on macOS/Linux.
   - Repo readiness: README with screenshots, "Why DupeLens?" (after trying Krokiet and dupeGuru),
     CONTRIBUTING, CODE_OF_CONDUCT, SECURITY.md, issue templates.
   - CI on Windows/Linux/macOS; releases via `dist`; winget + Homebrew ([0010](decisions/0010-distribution-plan.md)).
2. **v0.2: Phase 2 file types:** videos, PDFs, Office files, ZIPs, audio, code (exact matching), same flow.
3. **v0.3: Tauri desktop app** showing the v0.1 UI (`desktop/` already exists and is the main program); installers.
4. **Later: Android** via Tauri 2 (see blockers below). **Phase 3** AI similarity (CLIP/SigLIP/DINOv2).

Ideas: **watch mode** (flag new duplicates as files arrive), showing exact duplicates first
while look-alike analysis continues, scan cancellation.

## Open questions for the maintainer
- [0005](decisions/0005-recommendation-rules.md): should a favourite always be kept? Is the
  batch-action interpretation right?
- HEIC (iPhone) photos only match when byte-identical. Acceptable for now?
- Android Phase 2: request Google Play's restricted "All files access", or only work with
  files picked through the system picker?
- Which GitHub account and org hosts the public repo?

## Platform blockers (summary)
| Platform | Engine/CLI | App blockers |
|---|---|---|
| Windows | ✅ | Flutter path: MSVC Build Tools need admin. Tauri: builds locally without MSVC (Zig workarounds); release builds should still use MSVC on CI. |
| Linux | ✅ (verified in Debian container) | Must build on Linux (or WSL/CI). The app needs WebKitGTK (present on practically all desktop Linux; packaging declares it). |
| macOS | Expected ✅ (untested) | Needs a Mac or CI to build; Apple signing and notarisation. |
| Android | ❓ not compiled (needs NDK) | Deleting other apps' photos needs Android's system delete/trash requests (Kotlin side); "selected photos only" permission; long scans need a foreground service; Phase 2 needs a restricted permission or the system picker. |
