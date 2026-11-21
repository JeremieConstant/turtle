//! Opens the SQLite database (ADR-0004) and brings its schema up to date.

use anyhow::Context;
use rusqlite::Connection;
use std::path::Path;

/// Schema changes, applied in order on start. The database remembers how many
/// it has seen, so a migration is never edited once released: add a new one.
const MIGRATIONS: &[&str] = &[
    // Databases written by the first slice already have this table.
    "CREATE TABLE IF NOT EXISTS pages (id TEXT PRIMARY KEY, state BLOB NOT NULL)",
    "CREATE TABLE members (
         id TEXT PRIMARY KEY,
         email TEXT NOT NULL UNIQUE,
         name TEXT NOT NULL,
         password_hash TEXT NOT NULL,
         workspace_admin INTEGER NOT NULL,
         created_at INTEGER NOT NULL
     );
     CREATE TABLE sessions (
         id TEXT PRIMARY KEY,
         member_id TEXT NOT NULL REFERENCES members (id) ON DELETE CASCADE,
         created_at INTEGER NOT NULL,
         expires_at INTEGER NOT NULL
     );",
];

pub fn open(path: &Path) -> anyhow::Result<Connection> {
    let mut db =
        Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.pragma_update(None, "foreign_keys", true)?;

    let applied: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let applied = applied as usize;
    anyhow::ensure!(
        applied <= MIGRATIONS.len(),
        "{} was written by a newer version of Turtle",
        path.display()
    );
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(applied) {
        let transaction = db.transaction()?;
        transaction
            .execute_batch(migration)
            .with_context(|| format!("applying migration {}", index + 1))?;
        transaction.pragma_update(None, "user_version", index as i64 + 1)?;
        transaction.commit()?;
    }
    Ok(db)
}
