# Dev setup for building the Tauri app (desktop/) on Windows WITHOUT the MSVC Build Tools.
#   source scripts/desktop-env.sh
#   cd desktop && cargo build        # plain cargo, NOT cargo-zigbuild
#
# Why not `cargo-zigbuild build` (which works for the CLI)? Two problems, both in programs
# that run *during* the build (docs/DEVELOPMENT.md, "Building desktop/"):
# 1. With a --target (cargo-zigbuild always sets one), Cargo applies the linker setting only
#    to the app. Build scripts are linked by Rust's bundled GNU ld instead, and Tauri's build
#    script then crashes on start (0xc0000005).
# 2. cargo-zigbuild drops rustc's `-Wl,list.def` export list, so the tauri-macros DLL exports
#    69,808 symbols and fails Windows' 65,535 limit.
# Fix: no --target (so the linker setting covers build scripts and proc-macros too), and a
# wrapper (scripts/zig-cc.py) that passes the .def file in a form Zig accepts.
source "$(dirname "${BASH_SOURCE[0]}")/dev-env.sh"
unset CARGO_BUILD_TARGET
# cargo-zigbuild's `zig cc` only applies its MinGW argument fixes (-lmsvcrt, -l:libpthread.a)
# when it knows the Zig version; its own wrappers set these two, so ours must too.
export CARGO_ZIGBUILD_ZIG_COMMAND="$(cygpath -w "$(command -v zig)")"
export CARGO_ZIGBUILD_ZIG_VERSION="$(zig version)"
_scripts=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -W)
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$_scripts/zig-cc.cmd"
export CC_x86_64_pc_windows_gnu="$_scripts/zig-cc.cmd"
export CXX_x86_64_pc_windows_gnu="$_scripts/zig-cxx.cmd"
export AR_x86_64_pc_windows_gnu="$_scripts/zig-ar.cmd"
# rustc runs `dlltool.exe` to make import libraries (raw-dylib, used by windows-link).
# cargo-zigbuild acts as Zig's dlltool when started under that name, so put a copy on PATH.
_tools="$_scripts/../target/zig-tools"
mkdir -p "$_tools"
[ -f "$_tools/dlltool.exe" ] || cp "$(command -v cargo-zigbuild)" "$_tools/dlltool.exe"
# Tauri embeds the app icon via `windres`, which isn't installed; scripts/windres-zig.rs
# stands in for it using `zig rc`. Built once, with the linker wrapper above.
if [ ! -f "$_tools/windres.exe" ] || [ "$_scripts/windres-zig.rs" -nt "$_tools/windres.exe" ]; then
  rustc -O --edition 2021 -C linker="$_scripts/zig-cc.cmd" -o "$_tools/windres.exe" "$_scripts/windres-zig.rs"
fi
export PATH="$(cygpath -u "$_tools"):$PATH"
unset _tools
# Debug info makes each Tauri build script ~110 MB; skip it to save disk space.
export CARGO_PROFILE_DEV_DEBUG=0
unset _scripts
