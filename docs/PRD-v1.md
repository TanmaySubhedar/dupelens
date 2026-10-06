# Duplicate Photo Finder - Product Requirements Document (PRD)

**Version:** 1.0

## Vision

A privacy-first, offline-first, cross-platform duplicate finder that never uploads user files. All scanning, indexing, duplicate detection and deletion decisions occur locally.

## Platforms

- Android
- Windows
- macOS
- Linux
- iOS (planned)

**UI:** Flutter

**Core Engine:** Rust

---

# Roadmap

## Phase 1 – Smart Duplicate Cleaner (Images)

### Goals

- Detect exact duplicates using SHA-256.
- Detect near duplicates using pHash/dHash/aHash.
- Ask the user to select folders immediately on first launch.
- Never scan folders that were not selected.
- Group duplicates.
- Recommend the best image to retain.
- Allow one-click cleanup similar to Google Files.
- Entirely offline.

### First Launch

1. Welcome screen.
2. User selects one or more folders.
3. App remembers selected folders.
4. Scan begins.

### Detection Pipeline

1. SHA-256 → Confirmed duplicates.
2. pHash/dHash/aHash → Near duplicates.
3. Metadata verification.
4. Duplicate grouping.
5. Recommendation engine.

### Recommendation Criteria

Rank images using:

- Highest resolution
- Largest dimensions
- Better image quality
- Richest EXIF metadata
- Earliest original creation timestamp
- Favorite flag (if available)

### UI Principles

- Gallery-first design.
- No metadata-heavy interface.
- Each duplicate group displayed as a grid.
- Recommended image marked with a ⭐/👑 icon.
- Technical details shown only on demand.

### Smart Deletion Workflow

Default behavior:

- Keep the recommended image.
- Automatically pre-select every other image in the group for deletion.
- User can override any selection.

Batch actions:

- Keep Recommended (default)
- Review Before Deleting
- Clear Selection
- Restore Group
- Manual Selection

### Delete Flow

After pressing Delete:

- Move to Trash / Recycle Bin (default when supported)
- Permanently Delete

Nothing is deleted automatically.

---

## Phase 2 – Universal Duplicate Cleaner

Extend exact duplicate detection (hash-based) to:

- Videos
- PDFs
- Word documents
- Excel files
- ZIP archives
- Audio files
- Source code
- Other binary files

Reuse the same scan, grouping and deletion workflow.

---

## Phase 3 – AI Similarity Engine

Introduce semantic similarity using:

- CLIP
- SigLIP
- DINOv2

Capabilities:

- Similar photos
- Burst shots
- Edited versions
- Cropped images
- Same scene detection

Presented separately from confirmed duplicates.

---

# Architecture

Flutter UI

↓

Application Layer

↓

Rust Core

- Scanner
- Metadata Extractor
- Hash Engine
- Grouping Engine
- Recommendation Engine
- SQLite

---

# Core Principles

- Offline only
- Privacy first
- User-controlled folder selection
- Read-only until deletion confirmation
- Recommendations are advisory
- Never delete without explicit user confirmation
- Fast incremental rescans
