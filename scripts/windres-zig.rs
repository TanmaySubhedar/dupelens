//! A `windres.exe` stand-in that runs `zig rc`, for building desktop/ without MSVC or MinGW.
//!
//! Tauri embeds the app icon and version info through the `embed-resource` crate, which on
//! `*-windows-gnu` always runs a program named `windres` with GNU-style arguments:
//!
//!   windres --input app.rc --output libresource.a --include-dir DIR --output-format=coff
//!           --target pe-x86-64 -c 65001 [-D NAME[=VALUE]]... [-I DIR]...
//!
//! `zig rc` does the same job with Microsoft-style arguments, so this translates and forwards.
//! scripts/desktop-env.sh compiles it into target/zig-tools/windres.exe and puts it on PATH.

use std::process::{exit, Command};

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut input, mut output) = (None, None);
    let mut extra: Vec<String> = Vec::new();
    // Called with no arguments, or `-V`, only to check that it runs.
    let mut any = false;
    while let Some(a) = args.next() {
        any = true;
        let mut value = |name: &str| {
            args.next().unwrap_or_else(|| {
                eprintln!("windres-zig: {name} needs a value");
                exit(2)
            })
        };
        match a.as_str() {
            "--input" | "-i" => input = Some(value("--input")),
            "--output" | "-o" => output = Some(value("--output")),
            "--include-dir" | "-I" => extra.extend(["/i".into(), value("-I")]),
            "-D" | "--define" => extra.extend(["/d".into(), value("-D")]),
            "-c" | "--codepage" => extra.extend(["/c".into(), value("-c")]),
            "--target" | "-F" => {
                value("--target"); // always x86_64 here; see /:target below
            }
            "--output-format=coff" => {}
            "-V" | "--version" => {
                println!("windres-zig (zig rc wrapper)");
                return;
            }
            s if s.starts_with("-D") => extra.extend(["/d".into(), s[2..].into()]),
            s if s.starts_with("-I") => extra.extend(["/i".into(), s[2..].into()]),
            other => {
                eprintln!("windres-zig: unsupported argument {other}");
                exit(2)
            }
        }
    }
    if !any {
        return;
    }
    let (Some(input), Some(output)) = (input, output) else {
        eprintln!("windres-zig: --input and --output are required");
        exit(2)
    };
    let status = Command::new("zig")
        .args(["rc", "/:output-format", "coff", "/:target", "x86_64"])
        .args(["/:auto-includes", "gnu"])
        .args(&extra)
        .args(["/fo", &output, "--", &input])
        .status()
        .unwrap_or_else(|e| {
            eprintln!("windres-zig: cannot run zig: {e}");
            exit(1)
        });
    exit(status.code().unwrap_or(1));
}
