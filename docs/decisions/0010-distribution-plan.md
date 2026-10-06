# 0010: Distribution: GitHub Releases via CI, package managers, signing

- **Status:** Proposed
- **Date:** 2026-10-06

## Context
With the local web UI (0008), DupeLens ships as **one executable per platform**.
Audience: the public (open source). Android isn't covered by this plan.

## Decision (direction)
- **Build in the cloud with GitHub Actions** for Windows, Linux and macOS on every release.
  GitHub's Windows runners have the MSVC tools, which removes the local admin-rights
  problem for official builds (0002).
- Use **`dist`** (formerly `cargo-dist`) to generate the release workflow, installers and
  Homebrew packaging.
- Channels:
  - **Windows:** GitHub Releases zip, **winget**; later Scoop/Chocolatey, possibly the Microsoft Store.
  - **Linux:** `.tar.gz` (built for broad compatibility), `.deb`/`.rpm`, AppImage. Flatpak
    and Snap need extra sandbox permissions (folder access, trash).
  - **macOS:** Homebrew. Needs an Apple Developer ID and notarisation for smooth installs.
- **Code signing:** Windows via Azure Trusted Signing (cheapest legitimate route) or a
  certificate. Unsigned builds trigger SmartScreen warnings. macOS via a Developer ID.

## Open items
- No git remote exists yet. Decide which GitHub account and org hosts the public repo.
- Third-party license notices must ship with releases (dependencies are MIT/Apache;
  SQLite is public domain).
