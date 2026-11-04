# 0015: Deletion history, undo and the lifetime total

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Cleaning up is only comfortable if it can be undone, and people return to a tool that shows what
it has done for them (docs/UI.md, "Done" screen). Both need a memory of what was deleted. DupeLens
has no network and no account, so that memory is local.

## Decision
1. **A history table.** Every file deleted through DupeLens is recorded: when, where from, its
   size, and whether it went to the Recycle Bin (restorable) or was deleted permanently. Files
   deleted by one action share a **batch** number, returned to the page as `batch`.
2. **Undo restores a batch from the Recycle Bin**, using the OS trash (Windows and Linux; macOS
   has no restore API in the `trash` crate, so `undo_supported` is false there). Each file is
   looked up by its original path and a deletion time from that action, and restored on its own, so
   one failure does not stop the rest. Not restored, and reported with a plain reason:
   - the path is occupied (nothing is overwritten);
   - the file has left the bin (it was emptied).
   A permanent delete is refused with an explanation. After an undo the page rescans, because the
   restored files are not in the scan results.
3. **The lifetime total** is computed from the history: the size and count of everything deleted
   and not since restored, plus when the last clean-up was. Nothing is uploaded; it lives in the
   same database as everything else.
4. **Honest wording.** A file moved to the Recycle Bin counts as freed, but the disk space only
   returns when the bin is emptied. The Done screen says so.
5. **First-run suggestions** (`suggested_folders`, a host command): the user's Pictures, Downloads,
   Desktop, Documents, Videos and Music folders that exist, for one-click choices. They only
   offer paths; nothing is read until the user adds them and scans.

## Rejected
- **A "freed" total that includes undone deletions.** It would overstate what was achieved.
- **Our own trash folder instead of the system's.** The user already knows the Recycle Bin, and a
  private one could fill the disk silently.

## Known gaps
- Undo is by path: if a different file is created at the same path later, the original is not
  restored (reported, not overwritten).
- On macOS there is no undo; the files are in the Trash to restore by hand.
- The history is never trimmed.
- Undo picks the newest Recycle Bin entry for a path from this action's time onward. If the same
  path was deleted again later (the file recreated and deleted), the later copy can be the one
  restored.
- `stats.last_batch` is the most recent action not undone. If that was a permanent delete, an
  earlier Recycle Bin action is reachable only by passing its `batch` to `undo`.
- A failed history write after a file is deleted does not stop the report (the file is gone, so
  the report must say so); that file simply cannot be undone.
