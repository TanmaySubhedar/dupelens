# 0006: Deletion safety rules and canonical folder paths

- **Status:** Accepted
- **Date:** 2026-10-05

## Context
The PRD: "Read-only until deletion confirmation", "Never delete without explicit user
confirmation", "Recommendations are advisory". A duplicate cleaner that deletes the wrong
file loses someone's photos, so safety is enforced in the engine, not left to each UI.

## Decision (enforced in `Engine::delete`, tested in `core/tests/end_to_end.rs`)
- Only selected folders are walked; symbolic links are not followed out of them.
- Files are opened read-only during scanning.
- Recycle Bin / Trash is the default; permanent deletion must be chosen explicitly.
- The **whole request is refused** if any file id is unknown, or if it would delete
  **every copy** in a group.
- A file is **skipped** if its size or modification time changed since the scan, or if
  it's no longer inside a selected folder.
- **Folders are stored canonicalised** (true letter case, links and `subst` drives
  resolved). Found during development: if one folder were stored under two spellings
  (`C:\Photos` and `c:\photos`), every photo would be scanned twice, look like a duplicate
  *of itself*, and deleting the "copy" would delete the only file.

## Consequences
- UIs can't bypass these rules; they only choose which ids to send.
- Android needs a different delete path (system trash/delete requests), with the same
  checks run first; see the [ROADMAP](../ROADMAP.md).
