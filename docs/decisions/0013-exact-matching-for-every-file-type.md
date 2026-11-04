# 0013: Exact matching for every file type

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Until now the scanner admitted image files only. Users also have duplicate videos, music,
documents and archives, and those are where the big space savings are. The risk is cost: a
library of videos is hundreds of gigabytes, and hashing all of it on every scan would be slow.

## Decision
1. **Every regular file is scanned.** Photos keep the full look-alike analysis (0003, 0004).
   All other files take part in exact (SHA-256) matching only.
2. **Read as little as possible.** For a file that is not a photo:
   1. A file whose size no other scanned file shares cannot have an exact twin. It is recorded
      without a hash and never read.
   2. Files that share a size are compared by a fingerprint of their first and last 64 KiB
      (mixed with the length). Different fingerprints mean different files.
   3. Only files that still match, or whose size equals that of a file already hashed (a photo,
      or a cached hash), are read in full, as a stream, so a multi-gigabyte video never has to
      fit in memory.
3. **Cached by path, size and modified time**, as before. A file recorded without a hash is
   hashed later if another file of its size appears.
4. **Skipped on purpose:**
   - Windows system files and cloud-only placeholders (OneDrive "Files On-Demand"; reading one
     would download it). Their count is reported.
   - Hidden files and folders, Office lock files (`~$...`), `Thumbs.db`, `desktop.ini`.
   - Folders named `Windows`, `Program Files`, `Program Files (x86)`, `ProgramData`,
     `AppData`, `node_modules`, `$Recycle.Bin` and similar, below a selected folder (a folder
     the user selects directly is always scanned).
   - Empty files (a minimum size, default 1 byte, in `ScanOptions`).
5. **Which copy to keep.** The photo rules still apply first. A name that does not look like a
   copy (`report (1).pdf`, `Copy of report.pdf`, `report - Copy.pdf`, `report_copy.pdf`) now
   beats one that does; after that, the oldest file, then the shortest path.
6. **Categories** (Photo, Video, Music, Document, Archive, Other) come from the extension only,
   for the UI's filter chips and icons.
7. **Database version 2.** `files.sha256` may be empty. The file tables are only a cache, so
   an older database has them dropped and refilled by the next scan; selected folders are kept.

## Measured
2 GB test library: 12 videos of 100 MB with different sizes, plus a 400 MB file and its copy.
Debug build, files in the OS cache.

| | Time | Read in full |
|---|---|---|
| First scan | 0.49 s | 2 files, 800 MB (the duplicate pair only) |
| Rescan, nothing changed | 0.16 s | nothing |

On a cold disk the duplicate pair still has to be read once, which at typical SSD speeds takes
a few seconds per gigabyte pair and longer on a hard drive. Never reading everything else is
the saving.

## Rejected
- **Hashing every file.** Simple, but reads the whole library for no benefit.
- **A hash other than SHA-256 for large files** (such as BLAKE3). Faster, but a second hash
  would complicate the cache. Revisit only if measurements on real libraries show hashing, not
  disk speed, is the limit.
- **Byte-by-byte comparison after the hash.** SHA-256 collisions are not a practical risk.

## Known gaps
- Two paths to the same file (hard links, shortcuts) look like duplicates, and deleting one
  frees no space. Not yet detected.
- A new file whose size equals that of an already-hashed file is hashed in full without first
  checking the fingerprint, because the cached hash has no fingerprint to compare with.
- Progress counts files, not bytes, and a scan cannot yet be cancelled.
- Videos, documents and archives have no look-alike matching (re-encoded video, the same song at
  a different bitrate).
