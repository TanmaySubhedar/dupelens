"""C compiler / linker wrapper for building the Tauri app (desktop/) without MSVC.

Used by scripts/desktop-env.sh as the linker AND C compiler for every program Cargo builds,
including build scripts and proc-macros (see docs/DEVELOPMENT.md, "Building desktop/"). It
forwards everything to `cargo-zigbuild zig cc`, which does the usual MinGW-to-Zig argument
clean-up, with one fix applied first:

rustc passes a DLL's export list as `-Wl,<dir>/list.def`. cargo-zigbuild 0.23 skips that
argument on purpose (src/zig/linker_args.rs), so LLD falls back to exporting *every* symbol.
For a big proc-macro (tauri-macros: 69,808 symbols) that exceeds the Windows limit of 65,535
exports. Passing the same .def file as a plain input instead makes LLD export exactly what
rustc asked for (2 symbols for a proc-macro).

usage: python zig-cc.py cc|c++ <args...>
"""
import os
import shutil
import subprocess
import sys
import tempfile

TARGET = ["-g", "-fno-sanitize=all", "-target", "x86_64-windows-gnu"]


def fix(arg):
    """`-Wl,<path>.def` -> `<path>.def`. Works on raw and on GNU-escaped (\\\\) text alike."""
    body = arg[4:]
    if arg.startswith("-Wl,") and "," not in body and body.lower().endswith(".def"):
        return body
    return arg


def main():
    mode, args = sys.argv[1], sys.argv[2:]
    tmp = None
    out = []
    for a in args:
        if a.startswith("@") and os.path.isfile(a[1:]):
            # rustc's response file: one GNU-escaped argument per line. cargo-zigbuild only
            # cleans up response files named `linker-arguments`, so keep that name.
            tmp = tmp or tempfile.mkdtemp(prefix="zigcc-")
            with open(a[1:], encoding="utf-8") as f:
                lines = f.read().split("\n")
            path = os.path.join(tmp, "linker-arguments")
            with open(path, "w", encoding="utf-8", newline="\n") as f:
                f.write("\n".join(fix(line) for line in lines))
            out.append("@" + path)
        else:
            out.append(fix(a))
    try:
        cmd = ["cargo-zigbuild", "zig", mode, "--", *TARGET, *out]
        return subprocess.run(cmd).returncode
    finally:
        if tmp:
            shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
