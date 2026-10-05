//! SQLite storage: selected folders and the per-file analysis cache that makes
//! rescans incremental (unchanged files are never re-read).

use crate::error::Result;
use crate::metadata::Analysis;
use crate::model::FileRecord;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: i64 = 1;

pub fn open(path: &Path) -> Result<Connection> {
    init(Connection::open(path)?)
}

pub fn open_in_memory() -> Result<Connection> {
    init(Connection::open_in_memory()?)
}

fn init(conn: Connection) -> Result<Connection> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         CREATE TABLE IF NOT EXISTS folders (
             path     TEXT PRIMARY KEY,
             added_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS files (
             id        INTEGER PRIMARY KEY,
             path      TEXT NOT NULL UNIQUE,
             size      INTEGER NOT NULL,
             mtime_ns  INTEGER NOT NULL,
             sha256    TEXT NOT NULL,
             ahash     INTEGER,
             dhash     INTEGER,
             phash     INTEGER,
             width     INTEGER,
             height    INTEGER,
             sharpness REAL,
             exif_tags INTEGER NOT NULL DEFAULT 0,
             taken_at  TEXT,
             favorite  INTEGER NOT NULL DEFAULT 0,
             scan_id   INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS files_sha256 ON files(sha256);
         CREATE TABLE IF NOT EXISTS meta (
             key   TEXT PRIMARY KEY,
             value INTEGER NOT NULL
         );",
    )?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(conn)
}

// Paths are stored as UTF-8 text; the scanner skips anything that is not valid UTF-8.
fn path_str(path: &Path) -> &str {
    path.to_str().expect("scanner only admits UTF-8 paths")
}

pub fn add_folder(conn: &Connection, path: &Path) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO folders (path, added_at) VALUES (?1, strftime('%s','now'))",
        [path_str(path)],
    )?;
    Ok(())
}

pub fn remove_folder(conn: &Connection, path: &Path) -> Result<bool> {
    Ok(conn.execute("DELETE FROM folders WHERE path = ?1", [path_str(path)])? > 0)
}

pub fn folders(conn: &Connection) -> Result<Vec<PathBuf>> {
    let mut stmt = conn.prepare("SELECT path FROM folders ORDER BY path")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows
        .map(|r| r.map(PathBuf::from))
        .collect::<rusqlite::Result<_>>()?)
}

pub fn next_scan_id(conn: &Connection) -> Result<i64> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('scan_id', 1)
         ON CONFLICT(key) DO UPDATE SET value = value + 1",
        [],
    )?;
    Ok(
        conn.query_row("SELECT value FROM meta WHERE key = 'scan_id'", [], |r| {
            r.get(0)
        })?,
    )
}

/// What we remember about a file in order to decide whether it changed since the last scan.
pub struct Fingerprint {
    pub id: i64,
    pub size: u64,
    pub mtime_ns: i64,
}

pub fn fingerprints(conn: &Connection) -> Result<HashMap<PathBuf, Fingerprint>> {
    let mut stmt = conn.prepare("SELECT path, id, size, mtime_ns FROM files")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            PathBuf::from(r.get::<_, String>(0)?),
            Fingerprint {
                id: r.get(1)?,
                size: r.get::<_, i64>(2)? as u64,
                mtime_ns: r.get(3)?,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn mark_seen(conn: &Connection, id: i64, scan_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE files SET scan_id = ?1 WHERE id = ?2",
        params![scan_id, id],
    )?;
    Ok(())
}

pub fn upsert(
    conn: &Connection,
    path: &Path,
    size: u64,
    mtime_ns: i64,
    a: &Analysis,
    scan_id: i64,
) -> Result<()> {
    // u64 hashes are stored bit-for-bit in SQLite's signed 64-bit integers.
    let h =
        |f: fn(&crate::hashing::PerceptualHashes) -> u64| a.hashes.as_ref().map(|x| f(x) as i64);
    conn.execute(
        "INSERT INTO files (path, size, mtime_ns, sha256, ahash, dhash, phash, width, height,
                            sharpness, exif_tags, taken_at, favorite, scan_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(path) DO UPDATE SET
             size = excluded.size, mtime_ns = excluded.mtime_ns, sha256 = excluded.sha256,
             ahash = excluded.ahash, dhash = excluded.dhash, phash = excluded.phash,
             width = excluded.width, height = excluded.height, sharpness = excluded.sharpness,
             exif_tags = excluded.exif_tags, taken_at = excluded.taken_at,
             favorite = excluded.favorite, scan_id = excluded.scan_id",
        params![
            path_str(path),
            size as i64,
            mtime_ns,
            a.sha256,
            h(|x| x.ahash),
            h(|x| x.dhash),
            h(|x| x.phash),
            a.width,
            a.height,
            a.sharpness,
            a.exif.tag_count,
            a.exif.taken_at,
            a.exif.favorite,
            scan_id
        ],
    )?;
    Ok(())
}

/// Forgets files not seen in the given scan (deleted, moved, or in a folder no longer selected).
pub fn prune_unseen(conn: &Connection, scan_id: i64) -> Result<usize> {
    Ok(conn.execute("DELETE FROM files WHERE scan_id != ?1", [scan_id])?)
}

pub fn delete_file_row(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM files WHERE id = ?1", [id])?;
    Ok(())
}

const FILE_COLUMNS: &str = "id, path, size, mtime_ns, sha256, ahash, dhash, phash, width, height,
                            sharpness, exif_tags, taken_at, favorite";

fn file_from_row(r: &Row) -> rusqlite::Result<FileRecord> {
    let hash = |i| r.get::<_, Option<i64>>(i).map(|v| v.map(|x| x as u64));
    Ok(FileRecord {
        id: r.get(0)?,
        path: PathBuf::from(r.get::<_, String>(1)?),
        size: r.get::<_, i64>(2)? as u64,
        mtime_ns: r.get(3)?,
        sha256: r.get(4)?,
        ahash: hash(5)?,
        dhash: hash(6)?,
        phash: hash(7)?,
        width: r.get(8)?,
        height: r.get(9)?,
        sharpness: r.get(10)?,
        exif_tags: r.get(11)?,
        taken_at: r.get(12)?,
        favorite: r.get(13)?,
    })
}

pub fn all_files(conn: &Connection) -> Result<Vec<FileRecord>> {
    let mut stmt = conn.prepare(&format!("SELECT {FILE_COLUMNS} FROM files ORDER BY id"))?;
    let rows = stmt.query_map([], file_from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn file_by_id(conn: &Connection, id: i64) -> Result<Option<FileRecord>> {
    Ok(conn
        .query_row(
            &format!("SELECT {FILE_COLUMNS} FROM files WHERE id = ?1"),
            [id],
            file_from_row,
        )
        .optional()?)
}
