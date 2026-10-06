# Decision records

Each file here records one significant decision: the situation, what was decided, what
was rejected and why, and what follows from it. They exist so that anyone revisiting the
project (including its author months later) can understand *why* things are the way they
are without reading the code or old chats.

**Rules of thumb**
- One decision per file, numbered in order. Never renumber.
- Don't rewrite history: if a decision changes, add a new record and mark the old one
  `Superseded by NNNN`.
- Status is one of: **Accepted** (in effect), **Proposed** (agreed direction, not built),
  **Needs confirmation** (a guess that the maintainer should confirm), **Superseded**.

| # | Decision | Status | Date |
|---|---|---|---|
| [0001](0001-rust-core-flutter-ui.md) | Rust core engine; Flutter UI as specified by the PRD | Accepted (UI part on hold, see 0008) | 2026-10-05 |
| [0002](0002-windows-build-without-admin.md) | Build on Windows without admin rights: GNU Rust + Zig + cargo-zigbuild | Accepted | 2026-10-05 |
| [0003](0003-detection-pipeline.md) | Detection pipeline: SHA-256, perceptual hashes, BK-tree | Accepted | 2026-10-05 |
| [0004](0004-pixel-level-check.md) | Pixel-level check to reject look-alike false positives | Accepted | 2026-10-06 |
| [0005](0005-recommendation-rules.md) | How the "keep" recommendation is chosen | Needs confirmation (parts) | 2026-10-05 |
| [0006](0006-deletion-safety.md) | Deletion safety rules and canonical folder paths | Accepted | 2026-10-05 |
| [0007](0007-open-source-positioning-license-name.md) | Open source: positioning, MIT/Apache-2.0, keep the name DupeLens | Accepted | 2026-10-06 |
| [0008](0008-ui-local-web-first.md) | UI: local browser UI first (v0.1), Tauri later; Flutter code kept | Proposed | 2026-10-06 |
| [0009](0009-scan-performance.md) | "Not lazy": scan speed work and rejected optimisations | Accepted | 2026-10-06 |
| [0010](0010-distribution-plan.md) | Distribution: GitHub Releases via CI, package managers, signing | Proposed | 2026-10-06 |

See also: [ROADMAP](../ROADMAP.md) for what's next and open questions, and
[PRD-v1](../PRD-v1.md) for the original product requirements.
