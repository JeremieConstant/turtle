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
    // A Token is stored by a digest of its value. One limited to chosen Areas
    // lists them in `token_areas`; `all_areas` keeps a Token whose Areas were
    // all deleted from reaching every Area instead of none.
    "CREATE TABLE tokens (
         id TEXT PRIMARY KEY,
         member_id TEXT NOT NULL REFERENCES members (id) ON DELETE CASCADE,
         name TEXT NOT NULL,
         secret_digest TEXT NOT NULL UNIQUE,
         read_only INTEGER NOT NULL,
         all_areas INTEGER NOT NULL,
         expires_at INTEGER,
         last_used_at INTEGER,
         created_at INTEGER NOT NULL
     );
     CREATE INDEX tokens_by_member ON tokens (member_id);
     CREATE TABLE token_areas (
         token_id TEXT NOT NULL REFERENCES tokens (id) ON DELETE CASCADE,
         area_id TEXT NOT NULL REFERENCES areas (id) ON DELETE CASCADE,
         PRIMARY KEY (token_id, area_id)
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
    // An author of a Version may be an Agent acting through a Token of the
    // Member. Its name is kept as it was, so a revoked Token still shows who
    // wrote; '' means the Member themself, because NULL would not keep the
    // key unique.
    "CREATE TABLE version_authors_new (
         page_id TEXT NOT NULL,
         number INTEGER NOT NULL,
         member_id TEXT NOT NULL REFERENCES members (id) ON DELETE CASCADE,
         agent TEXT NOT NULL DEFAULT '',
         PRIMARY KEY (page_id, number, member_id, agent),
         FOREIGN KEY (page_id, number) REFERENCES versions (page_id, number) ON DELETE CASCADE
     );
     INSERT INTO version_authors_new (page_id, number, member_id)
         SELECT page_id, number, member_id FROM version_authors;
     DROP TABLE version_authors;
     ALTER TABLE version_authors_new RENAME TO version_authors;",
    // A standalone Agent has a name, Tokens and Grants of its own. A Token
    // belongs to either a Member or an Agent, and so does a Version's author.
    // The Token tables are rebuilt with their Areas set aside first, since
    // dropping `tokens` would take the rows of `token_areas` with it.
    "CREATE TABLE agents (
         id TEXT PRIMARY KEY,
         name TEXT NOT NULL,
         created_at INTEGER NOT NULL,
         deactivated_at INTEGER
     );
     CREATE TABLE tokens_new (
         id TEXT PRIMARY KEY,
         member_id TEXT REFERENCES members (id) ON DELETE CASCADE,
         agent_id TEXT REFERENCES agents (id) ON DELETE CASCADE,
         name TEXT NOT NULL,
         secret_digest TEXT NOT NULL UNIQUE,
         read_only INTEGER NOT NULL,
         all_areas INTEGER NOT NULL,
         expires_at INTEGER,
         last_used_at INTEGER,
         created_at INTEGER NOT NULL,
         CHECK ((member_id IS NULL) != (agent_id IS NULL))
     );
     INSERT INTO tokens_new (id, member_id, name, secret_digest, read_only, all_areas, expires_at, last_used_at, created_at)
         SELECT id, member_id, name, secret_digest, read_only, all_areas, expires_at, last_used_at, created_at FROM tokens;
     CREATE TABLE kept_token_areas AS SELECT token_id, area_id FROM token_areas;
     DROP TABLE token_areas;
     DROP TABLE tokens;
     ALTER TABLE tokens_new RENAME TO tokens;
     CREATE INDEX tokens_by_member ON tokens (member_id);
     CREATE INDEX tokens_by_agent ON tokens (agent_id);
     CREATE TABLE token_areas (
         token_id TEXT NOT NULL REFERENCES tokens (id) ON DELETE CASCADE,
         area_id TEXT NOT NULL REFERENCES areas (id) ON DELETE CASCADE,
         PRIMARY KEY (token_id, area_id)
     );
     INSERT INTO token_areas SELECT token_id, area_id FROM kept_token_areas;
     DROP TABLE kept_token_areas;
     CREATE TABLE version_authors_new (
         page_id TEXT NOT NULL,
         number INTEGER NOT NULL,
         member_id TEXT REFERENCES members (id) ON DELETE CASCADE,
         agent_id TEXT REFERENCES agents (id) ON DELETE CASCADE,
         agent TEXT NOT NULL DEFAULT '',
         CHECK ((member_id IS NULL) != (agent_id IS NULL)),
         FOREIGN KEY (page_id, number) REFERENCES versions (page_id, number) ON DELETE CASCADE
     );
     INSERT INTO version_authors_new (page_id, number, member_id, agent)
         SELECT page_id, number, member_id, agent FROM version_authors;
     DROP TABLE version_authors;
     ALTER TABLE version_authors_new RENAME TO version_authors;
     CREATE UNIQUE INDEX version_authors_once
         ON version_authors (page_id, number, coalesce(member_id, ''), coalesce(agent_id, ''), agent);",
    // A Personal Area belongs to one Member (ADR-0008); Members who already
    // exist get theirs here, new ones when their account is created. Its IDs
    // are made like those of any Area: 16 random bytes in lowercase hex.
    // Deleting a Member must not take their Personal Area with it, since its
    // content may still be handed over, so the reference does not cascade.
    "ALTER TABLE areas ADD COLUMN personal_member_id TEXT REFERENCES members (id);
     CREATE UNIQUE INDEX one_personal_area_per_member ON areas (personal_member_id)
         WHERE personal_member_id IS NOT NULL;
     INSERT INTO areas (id, name, created_at, personal_member_id)
         SELECT lower(hex(randomblob(16))), 'Personal', unixepoch(), id FROM members;
     INSERT INTO nodes (id, area_id, parent_id, kind, created_at)
         SELECT lower(hex(randomblob(16))), id, NULL, 'root', created_at
         FROM areas WHERE personal_member_id IS NOT NULL;",
    // A Tag exists only on the Pages that carry it, so it has no table of its
    // own and disappears with the last of them. It is stored in lower case.
    "CREATE TABLE page_tags (
         page_id TEXT NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
         tag TEXT NOT NULL,
         PRIMARY KEY (page_id, tag)
     );
     CREATE INDEX page_tags_by_tag ON page_tags (tag);",
    // A Quick Note is a Page that stays known as its Member's wherever it is
    // moved, so the mark is on the node rather than on the Personal Area.
    "ALTER TABLE nodes ADD COLUMN quick_note_member_id TEXT REFERENCES members (id);
     CREATE INDEX quick_notes_by_member ON nodes (quick_note_member_id, created_at)
         WHERE quick_note_member_id IS NOT NULL;",
    // The Trash marks the top of each deleted Subtree, and each deleted Area;
    // what is beneath stays where it was, so restoring puts it all back. Who
    // deleted it is kept like the author of a Version. Removing that Member
    // or Agent must not bring the content back, so the reference is cleared
    // instead of taking the row with it. When what an Entry in the Trash was
    // beneath is removed for good, the Entry moves to the top of its Area and
    // `was_beneath` keeps the titles of where it was, as a JSON array.
    "CREATE TABLE trash (
         node_id TEXT PRIMARY KEY REFERENCES nodes (id) ON DELETE CASCADE,
         trashed_at INTEGER NOT NULL,
         member_id TEXT REFERENCES members (id) ON DELETE SET NULL,
         agent_id TEXT REFERENCES agents (id) ON DELETE SET NULL,
         agent TEXT NOT NULL DEFAULT '',
         was_beneath TEXT
     );
     CREATE TABLE deleted_areas (
         area_id TEXT PRIMARY KEY REFERENCES areas (id) ON DELETE CASCADE,
         deleted_at INTEGER NOT NULL,
         member_id TEXT REFERENCES members (id) ON DELETE SET NULL,
         agent_id TEXT REFERENCES agents (id) ON DELETE SET NULL,
         agent TEXT NOT NULL DEFAULT ''
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
    fn tokens_with_their_areas_and_version_authors_survive_the_arrival_of_agents() {
        let data_dir = tempfile::tempdir().unwrap();
        let path = database_in(data_dir.path()).unwrap();
        {
            // As the database was before standalone Agents.
            let old = Connection::open(&path).unwrap();
            old.pragma_update(None, "foreign_keys", true).unwrap();
            let before_agents = 8;
            for migration in &MIGRATIONS[..before_agents] {
                old.execute_batch(migration).unwrap();
            }
            old.pragma_update(None, "user_version", before_agents as i64).unwrap();
            add_member(&old).unwrap();
            old.execute_batch(
                "INSERT INTO areas (id, name, created_at) VALUES ('a1', 'Engineering', 0);
                 INSERT INTO nodes (id, area_id, parent_id, kind, created_at) VALUES ('p1', 'a1', NULL, 'root', 0);
                 INSERT INTO tokens (id, member_id, name, secret_digest, read_only, all_areas, created_at)
                     VALUES ('t1', 'm1', 'Claude', 'digest', 0, 0, 0);
                 INSERT INTO token_areas (token_id, area_id) VALUES ('t1', 'a1');
                 INSERT INTO versions (page_id, number, markdown, recorded_at) VALUES ('p1', 1, 'One.', 0);
                 INSERT INTO version_authors (page_id, number, member_id, agent) VALUES ('p1', 1, 'm1', 'Claude');",
            )
            .unwrap();
        }

        let db = open(&path).unwrap();

        let token: (String, String) = db
            .query_row(
                "SELECT tokens.member_id, token_areas.area_id FROM tokens JOIN token_areas ON token_areas.token_id = tokens.id",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(token, ("m1".into(), "a1".into()));
        let author: (String, String) = db
            .query_row("SELECT member_id, agent FROM version_authors", [], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap();
        assert_eq!(author, ("m1".into(), "Claude".into()));
    }

    #[test]
    fn members_who_already_exist_get_a_personal_area_each() {
        let data_dir = tempfile::tempdir().unwrap();
        let path = database_in(data_dir.path()).unwrap();
        {
            // As the database was before Personal Areas.
            let old = Connection::open(&path).unwrap();
            // Personal Areas came with the tenth migration.
            let before_personal_areas = 9;
            for migration in &MIGRATIONS[..before_personal_areas] {
                old.execute_batch(migration).unwrap();
            }
            old.pragma_update(None, "user_version", before_personal_areas as i64).unwrap();
            add_member(&old).unwrap();
            old.execute_batch(
                "INSERT INTO members (id, email, name, password_hash, workspace_admin, created_at)
                     VALUES ('m2', 'grace@example.com', 'Grace', 'hash', 0, 0);
                 INSERT INTO areas (id, name, created_at) VALUES ('a1', 'Engineering', 0);
                 INSERT INTO nodes (id, area_id, parent_id, kind, created_at) VALUES ('r1', 'a1', NULL, 'root', 0);",
            )
            .unwrap();
        }

        let db = open(&path).unwrap();

        let mut statement = db
            .prepare(
                "SELECT areas.name, areas.personal_member_id, count(nodes.id), length(areas.id)
                 FROM areas JOIN nodes ON nodes.area_id = areas.id AND nodes.parent_id IS NULL
                 GROUP BY areas.id ORDER BY areas.personal_member_id",
            )
            .unwrap();
        let areas: Vec<(String, Option<String>, i64, i64)> = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            areas,
            [
                ("Engineering".into(), None, 1, 2),
                ("Personal".into(), Some("m1".into()), 1, 32),
                ("Personal".into(), Some("m2".into()), 1, 32),
            ]
        );
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
