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
4. **Tentative groups.** A group found only because the setting is looser than Balanced (some
   member is further from the suggested copy than Balanced allows) is flagged `tentative`. The
   page shows it as "Maybe" and does not pre-select anything in it for deletion. The safety
   rules of 0006 are unchanged.
5. **Cancel.** A scan or regroup can be stopped. Nothing is half-written (one transaction at the
   end). A scan stopped while reading keeps the files already read, so the next scan does not
   repeat the work, and forgets nothing. A single multi-gigabyte file stops mid-read.
6. **Progress in bytes** (per 1 MiB chunk) while reading, so one huge file still moves the bar.
7. **Hard links** (two names for the same data) are left out of a scan and counted. They look
   like duplicates, but deleting one frees no space. Only files that share a size are opened to
   check, so the cost is small.

## Rejected
- **One free "similarity" slider.** The four values do not move together, and a single number
  would hide what is being relaxed. Presets give the common choices; Custom gives the rest.
- **Regrouping on every scan only.** It would make trying a setting cost a full scan.

## Known gaps
- "Tentative" looks at hash distance only, not the pixel-change score.
- The Custom values have no screen in the old pages yet; the redesign (docs/UI.md) adds it.
- The "not a duplicate" list is not built.
