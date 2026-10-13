# 0002: Build on Windows without admin rights (GNU Rust + Zig + cargo-zigbuild)

- **Status:** Accepted (for local development on machines without the MSVC Build Tools)
- **Date:** 2026-10-05

## Context
The normal Windows Rust setup (`x86_64-pc-windows-msvc`) needs Microsoft's C++ Build
Tools. Installing them needs admin rights, which the development laptop doesn't have (two
attempts failed with "access denied", and no elevation prompt appeared). The disk was also
nearly full at the time (~2 GB free).

## Decision
Use the **GNU variant of Rust** (`x86_64-pc-windows-gnu`, ships its own linker), **Zig** as
a portable C compiler (SQLite is C), and **cargo-zigbuild**, which makes Zig do the C
compiling, the Windows import libraries and the linking (via LLVM's LLD).
`source scripts/dev-env.sh`, then `cargo-zigbuild test`.

## Rejected attempts (don't repeat them)
1. **Bundled GNU tools alone:** the bundled `gcc` can only link, not compile C, and the
   bundled `dlltool` needs an assembler (`as.exe`) that isn't shipped.
2. **A home-made shim** that swapped in Zig's `dlltool` and `cc`: it compiled, but the
   programs **crashed at startup**, because GNU `ld` mis-links LLVM-format import libraries
   (a known issue noted in rustc's source).
3. **`cargo install cargo-zigbuild`** on such a machine: the result is linked by the broken
   path and crashes too. Use the prebuilt binary (`pip install --no-deps cargo-zigbuild`).

## Consequences
- Local builds target `x86_64-pc-windows-gnu`; binaries land in `target/x86_64-pc-windows-gnu/`.
- Official Windows release builds should use MSVC on CI (GitHub's Windows runners have it);
  see 0010. Flutter on Windows would still need the MSVC tools.
- Machines with MSVC, and Linux/macOS, just use plain `cargo`.
- The Tauri app (`desktop/`) needs more than `cargo-zigbuild`: build scripts and proc-macros
  must be linked through Zig too, and three tools need stand-ins. That's `scripts/desktop-env.sh`
  (2026-10-08; details in DEVELOPMENT.md, "Building the Tauri app").
