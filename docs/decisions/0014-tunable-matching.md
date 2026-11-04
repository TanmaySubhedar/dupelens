# 0014: Tunable matching, with cancel, byte progress and hard links

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
What counts as a look-alike differs between people. Until now four numbers decided it
(0003, 0004) and nobody could change them. Re-running had to mean scanning again, and a
scan could not be stopped.

## Decision
1. **Four presets** stand for sets of the four values (plus a switch for look-alikes at all):

   | Preset | Overall look (pHash bits) | Fine detail (dHash bits) | Shape tolerance | Pixel change |
   |---|---|---|---|---|
   | Exact only | – (look-alikes off) | – | – | – |
   | Strict | 4 | 6 | 3% | 10 |
   | **Balanced** (default, today's behaviour) | 8 | 12 | 5% | 18 |
   | Loose | 12 | 18 | 8% | 30 |

   A fifth, **Custom**, holds the four raw values, checked against limits (24, 32, 25%, 60).
2. **Changing the setting never needs a rescan.** Fingerprints and pixel-comparison scores are
   cached, so a regroup compares only look-alike pairs not seen before. `start_regroup` runs it
   in the background with progress and cancel.
3. **Saved in the database** (a `settings` table), so it survives a restart. The minimum file
   size a scan includes is saved the same way.
4. **Tentative groups.** A group that exists only because the setting is looser than Balanced
   on **any** of the four tests (overall look, fine detail, shape, pixel change) is flagged
   `tentative`. It is worked out by also grouping with Balanced alone: a group built from more
   than one Balanced group needed a looser setting. The page shows it as "Maybe" and does not
   pre-select anything in it for deletion. A first version looked at the overall-look distance
   only, which let a pair joined by a looser pixel check through unflagged; a review caught it,
   and each of the four tests now has its own test.
5. **Cancel.** A scan or regroup can be stopped. Nothing is half-written (one transaction at the
   end). A scan stopped while reading keeps the files already read, so the next scan does not
   repeat the work, and forgets nothing. A single multi-gigabyte file stops mid-read.
6. **Progress in bytes** (per 1 MiB chunk) while reading, so one huge file still moves the bar.
7. **Hard links** (two names for the same data) are left out of a scan and counted. They look
   like duplicates, but deleting one frees no space. Only files that share a size are opened to
   check, so the cost is small.
8. **The kept copy is checked before deleting.** Besides the "never every copy" rule (0006), the
   engine now refuses to delete from a group unless at least one copy that stays still exists on
   disk with the size and modified time the last scan recorded. A cancelled scan forgets nothing,
   so a copy removed or edited outside DupeLens could otherwise be listed as the survivor.

## Rejected
- **One free "similarity" slider.** The four values do not move together, and a single number
  would hide what is being relaxed. Presets give the common choices; Custom gives the rest.
- **Regrouping on every scan only.** It would make trying a setting cost a full scan.

## Known gaps
- The Custom values have a screen (Settings) with range checks; it shows the engine's limits as numbers, with no live preview of what a change would find.
- The "not a duplicate" list is not built.
- Hard-link detection holds one open file handle per file that shares a size, at the same time.
  On Linux and macOS with very many same-size files it can hit the open-file limit and then
  silently skips detection for those files (the files are kept, so nothing is hidden). File ids
  on ReFS are 128-bit and compared as 64-bit.
- `cancel` is check-then-set, so a very late click could cancel the next job.
