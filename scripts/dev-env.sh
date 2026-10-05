# Dev setup for Windows machines WITHOUT the Microsoft C++ Build Tools (no admin rights needed).
#   source scripts/dev-env.sh
#   cargo-zigbuild test        # instead of `cargo test`
#   cargo-zigbuild run --bin dupelens -- --help
#
# Rust's windows-gnu toolchain cannot compile C (bundled SQLite) and its bundled GNU linker
# crashes with LLVM-made import libraries. cargo-zigbuild makes Zig do the C compiling,
# import-library generation and linking (via LLD) instead. See docs/DEVELOPMENT.md.
# Machines with the MSVC Build Tools can ignore this file and use plain `cargo`.
export PATH="$HOME/.cargo/bin:$PATH"
_zig_dir=$(ls -d "$HOME"/AppData/Local/Microsoft/WinGet/Packages/zig.zig_*/zig-x86_64-windows-* 2>/dev/null | head -1)
[ -n "$_zig_dir" ] && export PATH="$_zig_dir:$PATH"
unset _zig_dir
export CARGO_BUILD_TARGET=x86_64-pc-windows-gnu
