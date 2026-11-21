//! Opens the SQLite database (ADR-0004) and brings its schema up to date.

use anyhow::Context;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// Where the database lives in the data directory. Creates the directory if
/// this is the first start.
pub fn database_in(data_dir: &Path) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("creating the data directory {}", data_dir.display()))?;
    Ok(data_dir.join("turtle.db"))
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn add_member(db: &Connection) -> rusqlite::Result<usize> {
        db.execute(
            "INSERT INTO members (id, email, name, password_hash, workspace_admin, created_at)
             VALUES ('m1', 'ada@example.com', 'Ada', 'hash', 1, 0)",
            [],
        )
    }

    #[test]
    fn the_first_start_creates_the_data_directory_and_the_database_in_it() {
        let parent = tempfile::tempdir().unwrap();
        let data_dir = parent.path().join("not/yet/there");

        let path = database_in(&data_dir).unwrap();
        let db = open(&path).unwrap();

        assert_eq!(path, data_dir.join("turtle.db"));
        assert!(path.is_file());
        add_member(&db).expect("the schema is in place");
    }

    #[test]
    fn a_later_start_keeps_what_is_there() {
        let data_dir = tempfile::tempdir().unwrap();
        let path = database_in(data_dir.path()).unwrap();
        add_member(&open(&path).unwrap()).unwrap();

        let db = open(&database_in(data_dir.path()).unwrap()).unwrap();

        let name: String = db
            .query_row("SELECT name FROM members WHERE id = 'm1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(name, "Ada");
    }

    #[test]
    fn an_older_database_is_brought_up_to_date_on_start() {
        let data_dir = tempfile::tempdir().unwrap();
        let path = database_in(data_dir.path()).unwrap();
        {
            // What the first slice wrote, before migrations were numbered.
            let old = Connection::open(&path).unwrap();
            old.execute_batch(
                "CREATE TABLE pages (id TEXT PRIMARY KEY, state BLOB NOT NULL);
                 INSERT INTO pages (id, state) VALUES ('p1', x'0102');",
            )
            .unwrap();
        }

        let db = open(&path).unwrap();

        add_member(&db).expect("the newer tables were added");
        let state: Vec<u8> = db
            .query_row("SELECT state FROM pages WHERE id = 'p1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(state, vec![1, 2]);
    }

    #[test]
    fn a_database_from_a_newer_turtle_is_refused() {
        let data_dir = tempfile::tempdir().unwrap();
        let path = database_in(data_dir.path()).unwrap();
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", 9999)
            .unwrap();

        let error = open(&path).unwrap_err();

        assert!(
            error.to_string().contains("newer version of Turtle"),
            "{error}"
        );
    }
}
