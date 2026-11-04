# Changelog

## 0.2.0

### New
- **Every file type.** Besides photos, DupeLens now finds exact copies of videos, music,
  documents, archives and anything else. Files with a unique size are never read, and big files
  are compared in stages (size, then the first and last 64 KiB, then a full read only if needed).
  [Decision 0013](docs/decisions/0013-exact-matching-for-every-file-type.md)
- **You decide what counts as a look-alike.** Presets (Exact only, Strict, Balanced, Loose) and a
  Custom setting with four values. Changing it regroups your results in seconds without reading
  your folders again. Groups found only because the setting is looser than Balanced are marked
  "Maybe" and nothing in them is pre-selected for deletion.
  [Decision 0014](docs/decisions/0014-tunable-matching.md)
- **Undo.** Files moved to the Recycle Bin (Windows) or Trash (Linux) can be restored from the
  app, and DupeLens keeps a local running total of the space you have freed.
  [Decision 0015](docs/decisions/0015-deletion-history-and-undo.md)
- **Cancel** a scan or an update at any time; nothing is lost and files already read are kept.
  Progress is shown in bytes, so one huge file still moves the bar.
- **New interface and logo**, in light and dark. Every copy in every group shows its thumbnail,
  with a live preview pane and keyboard shortcuts (arrows, K keep, D delete, Space full size).
  Overview, Results, Review, Done and Settings screens.
- First run suggests your Pictures, Downloads, Desktop, Documents, Videos and Music folders.

### Safer
- A delete is refused unless a copy that stays still exists on disk, unchanged since the scan.
- Hard links (two names for the same data) are skipped, since deleting one frees no space.
- System files, cloud-only placeholders, hidden files and empty files are skipped.

### Changed
- The saved scan cache is rebuilt on first use after upgrading (your folders are kept).

### Not in this release
- Watch mode and HEIC support (planned for 0.3).
- Undo on macOS, a "not a duplicate" list, operating-system thumbnails for videos and PDFs, and
  the "found so far" panel during a scan.
