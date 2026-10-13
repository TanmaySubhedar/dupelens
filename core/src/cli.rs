//! The `dupelens` command-line commands, shared by the app (`desktop/`, where running
//! `dupelens <command>` in a terminal uses them) and the window-less `dupelens-cli`.
//! Each binary adds its own entry point; see `src/bin/dupelens-cli.rs`.

use crate::{trash_supported, DeleteMode, DuplicateGroup, Engine, GroupKind, ScanPhase};
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum Command {
    /// Manage which folders are scanned. Nothing outside these folders is ever read.
    Folders {
        #[command(subcommand)]
        action: FolderAction,
    },
    /// Scan the selected folders (incremental: unchanged files are not re-read).
    Scan,
    /// List duplicate groups found by the last scan.
    Groups {
        /// Print machine-readable JSON instead of a summary.
        #[arg(long)]
        json: bool,
        /// Show at most this many groups.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Delete duplicates, keeping the recommended copy in each group. Asks for confirmation.
    Clean {
        /// Only these groups (ids from `groups`). Default: all groups.
        #[arg(long = "group")]
        groups: Vec<i64>,
        /// Only exact (byte-identical) duplicates.
        #[arg(long)]
        exact_only: bool,
        /// Delete permanently instead of moving to the Recycle Bin / Trash.
        #[arg(long)]
        permanent: bool,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum FolderAction {
    List,
    Add { paths: Vec<PathBuf> },
    Remove { paths: Vec<PathBuf> },
}

/// `%LOCALAPPDATA%\DupeLens\dupelens.db` on Windows, the equivalent elsewhere.
pub fn default_db() -> Result<PathBuf> {
    let base = dirs::data_local_dir().context("cannot locate the app data folder; pass --db")?;
    Ok(base.join("DupeLens").join("dupelens.db"))
}

fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

/// Runs one command against an open engine.
pub fn run(engine: &mut Engine, command: Command) -> Result<()> {
    match command {
        Command::Folders { action } => folders(engine, action),
        Command::Scan => scan(engine),
        Command::Groups { json, limit } => groups(engine, json, limit),
        Command::Clean {
            groups,
            exact_only,
            permanent,
            yes,
        } => clean(engine, &groups, exact_only, permanent, yes),
    }
}

/// Opens the database at `db`, or at the per-user default location.
pub fn open_engine(db: Option<PathBuf>) -> Result<Engine> {
    let db = match db {
        Some(p) => p,
        None => default_db()?,
    };
    Engine::open(&db).with_context(|| format!("opening {}", db.display()))
}

fn folders(engine: &Engine, action: FolderAction) -> Result<()> {
    match action {
        FolderAction::List => {
            let list = engine.folders()?;
            if list.is_empty() {
                println!("No folders selected. Add one with: dupelens folders add <PATH>");
            }
            for f in list {
                println!("{}", f.display());
            }
        }
        FolderAction::Add { paths } => {
            for p in paths {
                println!("Added {}", engine.add_folder(&p)?.display());
            }
        }
        FolderAction::Remove { paths } => {
            for p in paths {
                if engine.remove_folder(&p)? {
                    println!("Removed {}", p.display());
                } else {
                    println!("Not selected: {}", p.display());
                }
            }
        }
    }
    Ok(())
}

fn scan(engine: &mut Engine) -> Result<()> {
    let summary = engine.scan(&|p| match p.phase {
        ScanPhase::Discovering => eprint!("\rLooking for photos..."),
        ScanPhase::Analyzing if p.total > 0 && (p.done % 25 == 0 || p.done == p.total) => {
            eprint!("\rAnalysing {}/{}        ", p.done, p.total)
        }
        ScanPhase::Saving => eprint!("\rSaving results...           "),
        _ => {}
    })?;
    eprintln!();
    println!(
        "{} photos found: {} analysed, {} unchanged since last scan, {} no longer present.",
        summary.files_found, summary.files_analyzed, summary.files_unchanged, summary.files_removed
    );
    for f in &summary.missing_folders {
        println!("Warning: folder not found (skipped): {}", f.display());
    }
    if !summary.unreadable.is_empty() {
        println!(
            "{} files could not be read and were skipped.",
            summary.unreadable.len()
        );
    }
    let groups = engine.groups()?;
    let reclaim: u64 = groups.iter().map(|g| g.reclaimable_bytes).sum();
    println!(
        "{} duplicate groups; {} can be freed. Run `dupelens groups` to review.",
        groups.len(),
        human_bytes(reclaim)
    );
    Ok(())
}

fn print_group(g: &DuplicateGroup) {
    let kind = match g.kind {
        GroupKind::Exact => "exact copies",
        GroupKind::Near => "look alike",
    };
    println!(
        "Group {} ({} files, {kind}, frees {}): keep because: {}",
        g.id,
        g.members.len(),
        human_bytes(g.reclaimable_bytes),
        g.reason
    );
    for m in &g.members {
        let mark = if m.file.id == g.recommended_id {
            "  KEEP  "
        } else {
            "  delete"
        };
        let dims = match (m.file.width, m.file.height) {
            (Some(w), Some(h)) => format!("{w}x{h}"),
            _ => "?".into(),
        };
        println!(
            "{mark}  {:>9}  {:>11}  {}",
            human_bytes(m.file.size),
            dims,
            m.file.path.display()
        );
    }
}

fn groups(engine: &Engine, json: bool, limit: Option<usize>) -> Result<()> {
    let mut groups = engine.groups()?;
    if let Some(n) = limit {
        groups.truncate(n);
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&groups)?);
        return Ok(());
    }
    if groups.is_empty() {
        println!("No duplicates found. (Did you run `dupelens scan`?)");
    }
    for g in &groups {
        print_group(g);
        println!();
    }
    Ok(())
}

fn clean(
    engine: &mut Engine,
    only: &[i64],
    exact_only: bool,
    permanent: bool,
    yes: bool,
) -> Result<()> {
    let wanted: HashSet<i64> = only.iter().copied().collect();
    let selected: Vec<DuplicateGroup> = engine
        .groups()?
        .into_iter()
        .filter(|g| wanted.is_empty() || wanted.contains(&g.id))
        .filter(|g| !exact_only || g.kind == GroupKind::Exact)
        .collect();
    if let Some(missing) = wanted
        .iter()
        .find(|id| !selected.iter().any(|g| g.id == **id))
    {
        bail!("no group with id {missing}; run `dupelens groups` to see current ids");
    }
    let ids: Vec<i64> = selected.iter().flat_map(|g| g.default_deletion()).collect();
    if ids.is_empty() {
        println!("Nothing to delete.");
        return Ok(());
    }

    let mode = if permanent || !trash_supported() {
        DeleteMode::Permanent
    } else {
        DeleteMode::Trash
    };
    let bytes: u64 = selected.iter().map(|g| g.reclaimable_bytes).sum();
    let verb = match mode {
        DeleteMode::Trash => "move to the Recycle Bin",
        DeleteMode::Permanent => "PERMANENTLY DELETE",
    };
    println!(
        "This will {verb} {} files ({}) from {} groups, keeping the recommended copy of each.",
        ids.len(),
        human_bytes(bytes),
        selected.len()
    );
    if !yes {
        print!("Type 'delete' to confirm: ");
        std::io::stdout().flush()?;
        let mut answer = String::new();
        std::io::stdin().lock().read_line(&mut answer)?;
        if answer.trim() != "delete" {
            println!("Cancelled. Nothing was deleted.");
            return Ok(());
        }
    }

    let report = engine.delete(&ids, mode)?;
    println!(
        "Deleted {} files, freed {}.",
        report.deleted.len(),
        human_bytes(report.bytes_freed)
    );
    for (id, why) in &report.failed {
        println!("Skipped file {id}: {why}");
    }
    Ok(())
}
