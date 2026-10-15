//! `dupelens-cli`: the same commands as the DupeLens app, without a window. For servers and
//! minimal Linux installs where the app can't start (it links against WebKitGTK for its
//! window), and for development. The main program is the app in `desktop/`.

use anyhow::Result;
use clap::Parser;
use dupelens_core::cli;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "dupelens-cli",
    version,
    about = "Find and clean up duplicate photos, entirely offline (no window; see `dupelens` for the app)."
)]
struct Args {
    /// Database file (defaults to the per-user app data folder).
    #[arg(long, global = true, env = "DUPELENS_DB")]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: cli::Command,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let (mut engine, _lock) = cli::open_engine(args.db)?;
    cli::run(&mut engine, args.command)
}
