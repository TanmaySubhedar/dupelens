# 0007: Open source: positioning, MIT/Apache-2.0 license, keep the name DupeLens

- **Status:** Accepted
- **Date:** 2026-10-06

## Context
The project is meant as an open-source contribution. The motivation: there's no desktop
equivalent of the duplicate cleaner in **Files by Google**, and Files itself is slow to
surface duplicates (it relies on background indexing) and doesn't let the user choose
which folders to scan.

## Decision
- **Positioning:** "Files by Google's duplicate cleaner for your desktop: open source and
  fully offline." The two headline promises:
  1. **You choose the folders**, and nothing else is read.
  2. **Results the moment you ask**: no background indexing to wait for (see 0009).
- **License:** dual **MIT OR Apache-2.0** (the Rust ecosystem norm, maximises adoption).
  Files: `LICENSE-MIT`, `LICENSE-APACHE`.
- **Name:** keep **DupeLens**. Free on crates.io; three small, inactive GitHub repos share
  the name (accepted risk). Alternatives considered: KeepBest, TwinSweep, PhotoWinnow.
- The maintainer confirmed there's no obstacle to open-sourcing the project.

## Landscape (why the positioning is about experience, not detection)
Open-source desktop duplicate finders already exist: **Czkawka / Krokiet** (Rust,
cross-platform, very capable), **dupeGuru**, **AntiDupl.NET**, **difPy**. Detection is
largely solved. The gap is the **opinionated one-tap experience**: recommend the keeper,
pre-select the rest, gallery-first, safe by default. Those tools are aimed at power users.
*To do:* try Krokiet and dupeGuru hands-on and write the "Why DupeLens?" README section.

## Consequences
- Contributions are dual-licensed by default (stated in the README).
- Before publishing: README screenshots, CONTRIBUTING, SECURITY.md (the app deletes files),
  CI on all platforms; see the [ROADMAP](../ROADMAP.md).
