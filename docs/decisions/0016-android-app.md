# 0016: Android app: a Kotlin plugin for the system calls, built in CI

- **Status:** Accepted for the approach (items 1 to 4). The two release flavours (item 5) are
  **Proposed** and still need the maintainer's confirmation.
- **Date:** 2026-11-05

## Context
A phone connected to a Windows PC by USB shows up as a device that speaks MTP (a protocol that
hands files over one at a time). It has no drive letter and no folder path, so the desktop app,
which works with ordinary paths, cannot scan it. Phone-folder scanning from the desktop was
looked at and **deliberately left out for now**; the real answer is an app that runs on the
phone. The roadmap already lists Android via Tauri 2 (0012).

Android only exposes some things through Java or Kotlin, not through plain file calls:
- trashing or deleting media that belongs to other apps (`MediaStore.createTrashRequest` and
  `createDeleteRequest`, which need a system confirmation screen launched from the app's
  activity), and putting trashed files back;
- a foreground service (a visible notification) so long scans are not killed;
- runtime permission prompts, and a media rescan so the gallery notices moved or deleted files;
- looking up a file's media ID, which the trash requests take instead of a path.

## Decision
1. **A separate Tauri plugin crate holds all the Android-specific calls.** The Rust side is
   a normal crate; its `android/` folder holds the Kotlin. It offers trash, untrash, the
   foreground notification, permission prompts, media rescan and media-ID lookup. On desktop it
   reports "not supported". We go fully with Kotlin for these calls rather than trying to reach
   Android from Rust through JNI (the Java bridge), which still needs the activity's result
   callback and is no less work.
2. **The engine stays in Rust and does not change for Android.** It reaches the plugin through
   a small "trash backend" interface (trash these files, restore these files). The desktop
   Recycle Bin, Android's own trash folder and Android's system trash all plug into that one
   interface. The safety rules (0006: never every copy, the kept copy must be unchanged) stay in
   the engine for every backend.
3. **The pages are reused, with a touch layout.** The desktop pages rely on pointing at a
   thumbnail, keyboard keys and a wide window. A phone needs tap to preview or toggle, and
   probably a bottom sheet instead of the side pane.
4. **Build in GitHub Actions, not on the maintainer's machine.** Android Studio is not
   needed to build: a JDK, the Android command-line tools (SDK platform and build tools), the
   NDK and the Rust target `aarch64-linux-android` are enough, and GitHub's Ubuntu runners carry
   the Android tooling. Local installation would cost roughly 4 to 6 GB (more with an emulator)
   on a machine that already struggles without MSVC and admin rights. The first CI milestone is
   a workflow that builds a **debug APK from the current pages**, installed on a real phone to
   see whether the pages load on Android at all.
5. **Two flavours, one engine and one plugin interface** *(proposed)*:

   | | APK (direct download, F-Droid) | Google Play |
   |---|---|---|
   | Access | "All files access", turned on by the user in Android settings | Photos and videos through the media library only |
   | Delete | Move to a hidden **DupeLens Trash** folder on the same storage: instant, copies nothing; Undo moves back; emptied on request or after some days | The system trash (`createTrashRequest`), which keeps files about 30 days; Undo untrashes |
   | Review | None | Permission declarations; the review is the main risk |
   | Scope of v1 | Whole storage | Photos and videos |

   Permanent delete without any trash was **rejected** (below).

## Rejected
- **Permanent delete only, positioned as an APK.** It is possible and avoids the Play review,
   but photos are often irreplaceable and the phone is often their only copy. One bug or wrong
   tap would be unrecoverable, against the promises of 0006 and 0015. Our own trash folder keeps
   Undo at little cost (space is freed only when it is emptied, as with a Recycle Bin).
- **Calling Android from Rust through JNI**, and **relying on a community plugin** for trash
   requests: no less work, or unknown coverage.
- **Scanning the phone from the desktop over MTP** (Windows' device API): slow, one transfer per
  file, the kept-copy check cannot be done the same way, and deletes on a phone have no
  Recycle Bin. Not planned.

## Facts to verify before relying on them
These are from general knowledge, not checked in this project, and Google changes them often.
- The size of a command-line-only Android toolchain (estimated 4 to 6 GB).
- That `tauri android build` works in CI as documented, and whether the existing `thumb://`
  image scheme works inside the Android webview.
- Google Play: a one-time registration fee (about $25); personal accounts created recently need a
  closed test (I recall at least 12 testers for 14 days) before production; new apps are
  submitted as an AAB (the bundle format), signed with Play App Signing; a required target
  Android API level that rises each year; a privacy policy URL; a Data safety form; and a
  content rating. "All files access" and the photo and video permissions each need a
  declaration and review, and a duplicate cleaner may not count as an allowed core use.
- Google's announced developer-verification requirement for apps installed outside the store,
  rolling out during 2026, may affect the direct-APK route.

## Steps, in order
1. Add a CI workflow that builds a debug APK of the current pages, and install it on a phone.
2. Build the plugin: permission prompt and foreground notification first (easiest to test), then
   trash and untrash, media-ID lookup and the media rescan.
3. Add the trash-backend interface to the engine, with the desktop Recycle Bin as its first
   implementation, then the Android one.
4. Touch layout for the pages.
5. Decide the flavours (item 5), then: for Play, the closed test, listing and permission
   declarations; for the APK, signing and a download page.

## Known gaps
- The engine has never been compiled for Android (no NDK has been installed). The bundled
  SQLite is C code and needs it.
- Phone-sized limits are unmeasured: battery, and scan speed with tens of thousands of photos.
- HEIC photos still match only when byte-identical (0003); the Android app will meet many.
