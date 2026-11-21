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
/// A migration is known by its position alone. When two branches each add
/// one, whoever merges second moves theirs after the other's.
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
    // An Area's tree hangs from a root node without a parent. Grants are given
    // on nodes, so a Subtree can later carry its own (Inheritance).
    "CREATE TABLE areas (
         id TEXT PRIMARY KEY,
         name TEXT NOT NULL,
         created_at INTEGER NOT NULL
     );
     CREATE TABLE nodes (
         id TEXT PRIMARY KEY,
         area_id TEXT NOT NULL REFERENCES areas (id) ON DELETE CASCADE,
         parent_id TEXT REFERENCES nodes (id) ON DELETE CASCADE,
         kind TEXT NOT NULL,
         created_at INTEGER NOT NULL
     );
     CREATE UNIQUE INDEX one_root_per_area ON nodes (area_id) WHERE parent_id IS NULL;
     CREATE INDEX nodes_by_parent ON nodes (parent_id);
     CREATE TABLE grants (
         node_id TEXT NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
         holder TEXT NOT NULL,
         role TEXT NOT NULL CHECK (role IN ('read', 'write', 'admin')),
         PRIMARY KEY (node_id, holder)
     );",
    // Pages and Folders are nodes too. Siblings are kept in the order of
    // their position; titles need not be unique. A Page's content stays in
    // `pages`, under the ID of its node.
    "ALTER TABLE nodes ADD COLUMN title TEXT NOT NULL DEFAULT '';
     ALTER TABLE nodes ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
     CREATE INDEX nodes_in_order ON nodes (parent_id, position);",
    // A deactivated Member keeps their account but cannot sign in. Links for
    // invitations and password resets are stored by a digest of their secret,
    // and deleted once used.
    "ALTER TABLE members ADD COLUMN deactivated_at INTEGER;
     CREATE TABLE single_use_links (
         id TEXT PRIMARY KEY,
         kind TEXT NOT NULL CHECK (kind IN ('invitation', 'password_reset')),
         email TEXT,
         member_id TEXT REFERENCES members (id) ON DELETE CASCADE,
         created_at INTEGER NOT NULL,
         expires_at INTEGER NOT NULL
     );",
    // A Version keeps the Page's content as Markdown, which holds everything
    // a Page can say (ADR-0006) and is what Versions are compared by.
    "CREATE TABLE versions (
         page_id TEXT NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
         number INTEGER NOT NULL,
         markdown TEXT NOT NULL,
         recorded_at INTEGER NOT NULL,
         restored_from INTEGER,
         PRIMARY KEY (page_id, number)
     );
     CREATE TABLE version_authors (
         page_id TEXT NOT NULL,
         number INTEGER NOT NULL,
         member_id TEXT NOT NULL REFERENCES members (id) ON DELETE CASCADE,
         PRIMARY KEY (page_id, number, member_id),
         FOREIGN KEY (page_id, number) REFERENCES versions (page_id, number) ON DELETE CASCADE
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
