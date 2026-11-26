//! What each import created, kept so it can be looked at and undone later.
//!
//! An import is recorded before it creates anything, and each Page and
//! Folder as it is created, so one that ended part way can be undone too.
//! Undoing moves what it created to the Trash, from where each can be
//! restored as anything else there. The report on an import stays, marked
//! as undone, so how the import went can still be read.

use crate::ops::permissions::{self, Role};
use crate::ops::versions::{stored_author, Author};
use crate::ops::{Catalogue, Core, OpError, OpResult, Principal};
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "list_imports",
            "Lists the imports into an Area, newest first: where each came from, when and by whom it was started, how many of the Pages and Folders it created are still there for the caller to see, the Page holding its report if it has one, whether the caller may undo it, and whether it was undone. Needs the Read Role on the Area.",
            list_imports,
        )
        .changes = false;
    catalogue.add(
        "undo_import",
        "Undoes an import: moves every Page and Folder it created that is still there to the Trash, from where each can be restored. What was created beneath them since goes to the Trash with them and is named in the answer. Its report stays, marked as undone. An import is undone once, by someone who may change all that it created.",
        undo_import,
    );
}

/// The Area whose imports to list.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ImportsOfArea {
    pub area_id: String,
}

/// The import to undo.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ImportToUndo {
    /// The ID list_imports, or the import itself, gave.
    pub import_id: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ImportList {
    pub imports: Vec<ImportRecord>,
}

/// An import as it is kept.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ImportRecord {
    pub id: String,
    /// What was imported, such as "the Confluence space Knowledge Base".
    pub source: String,
    /// Seconds since the Unix epoch.
    pub started_at: i64,
    /// Who started it; absent once they no longer exist.
    pub started_by: Option<Author>,
    /// How many of the Pages and Folders it created, its report aside, are
    /// still there, not in the Trash, for the caller to see.
    pub created_count: u32,
    /// The Page holding its report, while the caller may read it.
    pub report_page_id: Option<String>,
    /// Whether the caller may undo it: it is not undone yet, and they may
    /// change all that it created.
    pub undoable: bool,
    /// When it was undone, in seconds since the Unix epoch; absent if not.
    pub undone_at: Option<i64>,
}

/// What undoing an import did.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Undone {
    /// How many of the Pages and Folders the import created went to the Trash.
    pub trashed: u32,
    /// The titles of the Pages and Folders created beneath them since the
    /// import, which went to the Trash with them; those the caller may not
    /// read are left out.
    pub taken_along: Vec<String>,
}

/// A new import's ID.
pub(super) fn new_id() -> Result<String, OpError> {
    crate::ops::accounts::random_hex(16)
}

/// Records an import before it creates anything.
pub(super) fn begin(core: &Core, principal: &Principal, import_id: &str, area_id: &str, source: &str) -> Result<(), OpError> {
    let by = crate::ops::tokens::author_of(core, principal)?;
    let (member_id, agent_id, agent) = by.stored();
    core.db().execute(
        "INSERT INTO imports (id, area_id, source, started_at, member_id, agent_id, agent) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![import_id, area_id, source, core.now(), member_id, agent_id, agent],
    )?;
    Ok(())
}

/// Records a Page or Folder the import created.
pub(super) fn created(core: &Core, import_id: &str, node_id: &str) -> Result<(), OpError> {
    core.db().execute("INSERT INTO import_entries (import_id, node_id) VALUES (?1, ?2)", [import_id, node_id])?;
    Ok(())
}

/// Records the Page holding the import's report.
pub(super) fn report(core: &Core, import_id: &str, page_id: &str) -> Result<(), OpError> {
    core.db().execute("UPDATE imports SET report_page_id = ?2 WHERE id = ?1", [import_id, page_id])?;
    Ok(())
}

pub fn list_imports(core: &Core, principal: &Principal, input: ImportsOfArea) -> OpResult<ImportList> {
    crate::ops::areas::signed_in(principal)?;
    let db = core.db();
    crate::ops::areas::readable_area(&db, principal, &input.area_id)?;
    let mut statement = db.prepare(
        "SELECT imports.id, imports.source, imports.started_at, imports.report_page_id, imports.undone_at,
                members.id, members.name, imports.agent, agents.id, agents.name
         FROM imports
         LEFT JOIN members ON members.id = imports.member_id
         LEFT JOIN agents ON agents.id = imports.agent_id
         WHERE imports.area_id = ?1
         ORDER BY imports.started_at DESC, imports.rowid DESC",
    )?;
    let rows = statement
        .query_map([&input.area_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get(1)?, row.get(2)?, row.get::<_, Option<String>>(3)?, row.get::<_, Option<i64>>(4)?, stored_author(row, 5)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut imports = Vec::new();
    for (id, source, started_at, report_page_id, undone_at, started_by) in rows {
        let plan = plan(&db, principal, &id, report_page_id.as_deref())?;
        let report_page_id = match report_page_id {
            Some(page_id) if permissions::role_on(&db, principal, &page_id)?.is_some() => Some(page_id),
            _ => None,
        };
        let undoable = undone_at.is_none() && plan.may_undo;
        imports.push(ImportRecord { id, source, started_at, started_by, created_count: plan.visible, report_page_id, undoable, undone_at });
    }
    Ok(ImportList { imports }.into())
}

pub fn undo_import(core: &Core, principal: &Principal, input: ImportToUndo) -> OpResult<Undone> {
    crate::ops::areas::signed_in(principal)?;
    let not_found = || OpError::NotFound("There is no such import.".into());
    let (plan, report_page_id, taken_along) = {
        let db = core.db();
        let (area_id, undone_at, report_page_id): (String, Option<i64>, Option<String>) = db
            .query_row("SELECT area_id, undone_at, report_page_id FROM imports WHERE id = ?1", [&input.import_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .optional()?
            .ok_or_else(not_found)?;
        match crate::ops::areas::readable_area(&db, principal, &area_id) {
            Err(OpError::NotFound(_)) => return Err(not_found()),
            other => other?,
        };
        if undone_at.is_some() {
            return Err(undone_already());
        }
        let plan = plan(&db, principal, &input.import_id, report_page_id.as_deref())?;
        if !plan.may_undo {
            return Err(OpError::NotPermitted(
                "Only someone with the Write Role on all that this import created can undo it.".into(),
            ));
        }
        let created: HashSet<&str> = plan.remaining.iter().map(String::as_str).collect();
        let mut taken_along = Vec::new();
        for top in &plan.tops {
            for (id, title) in live_beneath(&db, top)? {
                if !created.contains(id.as_str()) && permissions::role_on(&db, principal, &id)?.is_some() {
                    taken_along.push(title);
                }
            }
        }
        // Claimed before anything moves, so an import is undone once even
        // when two people undo it at the same moment.
        let claimed = db.execute(
            "UPDATE imports SET undone_at = ?2 WHERE id = ?1 AND undone_at IS NULL",
            rusqlite::params![input.import_id, core.now()],
        )?;
        if claimed == 0 {
            return Err(undone_already());
        }
        (plan, report_page_id, taken_along)
    };
    for id in &plan.tops {
        match crate::ops::trash::move_to_trash(core, principal, crate::ops::trash::EntryId { id: id.clone() }) {
            // Moved or removed by someone else in the meantime: it is not
            // there to undo any more.
            Ok(_) | Err(OpError::NotFound(_)) => {}
            Err(error) => return Err(error),
        }
    }
    if let Some(page_id) = report_page_id {
        mark_undone(core, principal, &page_id)?;
    }
    Ok(Undone { trashed: plan.visible, taken_along }.into())
}

fn undone_already() -> OpError {
    OpError::Conflict("This import is undone already. What it created is in the Trash, from where it can be restored.".into())
}

/// Puts a note at the top of an import's report that the import was undone,
/// if the caller may change the report.
fn mark_undone(core: &Core, principal: &Principal, page_id: &str) -> Result<(), OpError> {
    use crate::ops::content::{get_page_markdown, replace_page, NewContent, OfPage};
    let markdown = match get_page_markdown(core, principal, OfPage { page_id: page_id.into() }) {
        Ok(read) => read.output.markdown,
        Err(OpError::NotFound(_)) => return Ok(()),
        Err(error) => return Err(error),
    };
    let note = format!(
        "> [!WARNING]\n> This import was undone on {}. What it created is in the Trash, from where each Page and Folder can be restored.\n\n",
        date_of(core.now())
    );
    match replace_page(core, principal, NewContent { page_id: page_id.into(), markdown: note + &markdown }) {
        Ok(_) | Err(OpError::NotFound(_) | OpError::NotPermitted(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

/// What undoing an import would move to the Trash, as one caller sees it,
/// and whether they may. This is where who may undo an import is decided.
struct Plan {
    /// What the import created, its report aside, that is still there and
    /// not in the Trash.
    remaining: Vec<String>,
    /// Of those, the ones with nothing the import created above them: they
    /// go to the Trash, and what is beneath them with them.
    tops: Vec<String>,
    /// How many of `remaining` the caller may read.
    visible: u32,
    /// Whether the caller holds Write on every one of `tops`.
    may_undo: bool,
}

fn plan(db: &Connection, principal: &Principal, import_id: &str, report_page_id: Option<&str>) -> Result<Plan, OpError> {
    let mut statement = db.prepare("SELECT node_id FROM import_entries WHERE import_id = ?1 ORDER BY rowid")?;
    let ids = statement.query_map([import_id], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    let mut remaining = Vec::new();
    for id in ids {
        if Some(id.as_str()) != report_page_id && !permissions::in_trash(db, &id)? {
            remaining.push(id);
        }
    }
    let created: HashSet<&str> = remaining.iter().map(String::as_str).collect();
    let (mut tops, mut visible, mut may_undo) = (Vec::new(), 0, true);
    for id in &remaining {
        let role = permissions::role_on(db, principal, id)?;
        visible += u32::from(role.is_some());
        if !has_created_above(db, id, &created)? {
            may_undo &= role >= Some(Role::Write);
            tops.push(id.clone());
        }
    }
    Ok(Plan { remaining, tops, visible, may_undo })
}

/// Whether one of `created` is above the node.
fn has_created_above(db: &Connection, id: &str, created: &HashSet<&str>) -> Result<bool, OpError> {
    let mut statement = db.prepare(
        "WITH RECURSIVE upward (id, parent_id, depth) AS (
             SELECT id, parent_id, 0 FROM nodes WHERE id = ?1
             UNION ALL
             SELECT nodes.id, nodes.parent_id, upward.depth + 1 FROM nodes JOIN upward ON nodes.id = upward.parent_id
         )
         SELECT id FROM upward WHERE depth > 0",
    )?;
    let above = statement.query_map([id], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(above.iter().any(|id| created.contains(id.as_str())))
}

/// The Pages and Folders beneath a node that are not in the Trash, with
/// their titles.
fn live_beneath(db: &Connection, id: &str) -> Result<Vec<(String, String)>, OpError> {
    let mut statement = db.prepare(
        "WITH RECURSIVE downward (id, title) AS (
             SELECT id, title FROM nodes WHERE parent_id = ?1 AND id NOT IN (SELECT node_id FROM trash)
             UNION ALL
             SELECT nodes.id, nodes.title FROM nodes JOIN downward ON nodes.parent_id = downward.id
             WHERE nodes.id NOT IN (SELECT node_id FROM trash)
         )
         SELECT id, title FROM downward",
    )?;
    let rows = statement.query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// The date of a moment, in seconds since the Unix epoch, as `YYYY-MM-DD`
/// in UTC.
pub(super) fn date_of(seconds: i64) -> String {
    // Days to a civil date, after Howard Hinnant's `civil_from_days`.
    let days = seconds.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::import::{import, ToImport};
    use crate::ops::testing::{fresh, give_role, member};
    use crate::ops::tree::{create_page, get_tree, Entry, NewEntry, TreeOfArea};
    use base64::Engine;
    use std::io::Write;

    fn zip_of(files: &[(&str, &str)]) -> String {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (path, content) in files {
            zip.start_file(*path, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        base64::engine::general_purpose::STANDARD.encode(zip.finish().unwrap().into_inner())
    }

    fn titles(core: &Core, reader: &Principal, area_id: &str) -> Vec<String> {
        fn walk(entries: &[Entry], out: &mut Vec<String>) {
            for entry in entries {
                out.push(entry.title.clone());
                walk(&entry.children, out);
            }
        }
        let mut out = Vec::new();
        walk(&get_tree(core, reader, TreeOfArea { area_id: area_id.into() }).unwrap().output.entries, &mut out);
        out
    }

    fn imports(core: &Core, reader: &Principal, area_id: &str) -> Vec<ImportRecord> {
        list_imports(core, reader, ImportsOfArea { area_id: area_id.into() }).unwrap().output.imports
    }

    /// An Area "Engineering" holding the Page "Notes" made by hand and an
    /// import of "Runbook" with "Deploy" beneath it, by Grace.
    fn imported() -> (crate::ops::testing::TestCore, Principal, String) {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = create_area(&core, &grace, NewArea { name: "Engineering".into() }).unwrap().output.id;
        let notes = NewEntry { area_id: area_id.clone(), parent_id: None, title: "Notes".into() };
        create_page(&core, &grace, notes.into()).unwrap();
        let content = zip_of(&[("Runbook.md", "Restart it."), ("Runbook/Deploy.md", "Ship it.")]);
        import(&core, &grace, ToImport { area_id: area_id.clone(), parent_id: None, content }).unwrap();
        (core, grace, area_id)
    }

    #[test]
    fn an_import_is_kept_with_what_it_created_and_who_started_it() {
        let (core, grace, area_id) = imported();

        let imports = imports(&core, &grace, &area_id);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source, "a ZIP of Markdown files");
        assert_eq!(imports[0].created_count, 2);
        assert!(imports[0].undoable);
        assert!(matches!(&imports[0].started_by, Some(Author::Member { name, .. }) if name == "Grace"));
        assert_eq!((imports[0].report_page_id.as_deref(), imports[0].undone_at), (None, None));
    }

    #[test]
    fn undoing_an_import_moves_what_it_created_to_the_trash_and_leaves_the_rest() {
        let (core, grace, area_id) = imported();
        let import_id = imports(&core, &grace, &area_id)[0].id.clone();

        let undone = undo_import(&core, &grace, ImportToUndo { import_id: import_id.clone() }).unwrap().output;

        assert_eq!((undone.trashed, undone.taken_along.len()), (2, 0));
        assert_eq!(titles(&core, &grace, &area_id), ["Notes"]);
        let trash = crate::ops::trash::get_trash(&core, &grace, crate::ops::trash::TrashOfArea { area_id: area_id.clone() }).unwrap().output;
        assert_eq!(trash.entries.iter().map(|entry| entry.title.as_str()).collect::<Vec<_>>(), ["Runbook"]);
        let after = &imports(&core, &grace, &area_id)[0];
        assert_eq!((after.created_count, after.undoable), (0, false));
        assert!(after.undone_at.is_some());
        let again = undo_import(&core, &grace, ImportToUndo { import_id }).unwrap_err();
        assert_eq!(again.kind(), "conflict");
    }

    #[test]
    fn what_was_added_beneath_an_imported_page_since_is_named_when_it_goes_along() {
        let (core, grace, area_id) = imported();
        let runbook = get_tree(&core, &grace, TreeOfArea { area_id: area_id.clone() }).unwrap().output.entries[1].id.clone();
        let mine = NewEntry { area_id: area_id.clone(), parent_id: Some(runbook), title: "My checklist".into() };
        create_page(&core, &grace, mine.into()).unwrap();
        let import_id = imports(&core, &grace, &area_id)[0].id.clone();

        let undone = undo_import(&core, &grace, ImportToUndo { import_id }).unwrap().output;

        assert_eq!(undone.taken_along, ["My checklist"]);
    }

    #[test]
    fn only_someone_with_write_undoes_and_imports_of_an_area_one_cannot_read_are_hidden() {
        let (core, grace, area_id) = imported();
        let ken = member(&core, "Ken", false);
        let ada = member(&core, "Ada", false);
        give_role(&core, &area_id, &ken, Role::Read);
        let import_id = imports(&core, &grace, &area_id)[0].id.clone();

        let refused = undo_import(&core, &ken, ImportToUndo { import_id: import_id.clone() }).unwrap_err();
        let hidden = undo_import(&core, &ada, ImportToUndo { import_id }).unwrap_err();
        let unlisted = list_imports(&core, &ada, ImportsOfArea { area_id: area_id.clone() }).unwrap_err();

        assert_eq!((refused.kind(), hidden.kind(), unlisted.kind()), ("not_permitted", "not_found", "not_found"));
        let seen_by_ken = imports(&core, &ken, &area_id);
        assert_eq!((seen_by_ken.len(), seen_by_ken[0].undoable), (1, false));
        assert_eq!(titles(&core, &grace, &area_id), ["Notes", "Runbook", "Deploy"]);
    }

    #[test]
    fn someone_with_write_on_only_the_part_imported_into_can_undo_their_import() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area_id = create_area(&core, &grace, NewArea { name: "Engineering".into() }).unwrap().output.id;
        give_role(&core, &area_id, &ken, Role::Read);
        let folder = crate::ops::tree::create_folder(&core, &grace, NewEntry { area_id: area_id.clone(), parent_id: None, title: "Ken's".into() }).unwrap().output.id;
        let Principal::Member { id: ken_id, .. } = &ken else { panic!("Ken is a Member") };
        permissions::grant(&core.db(), &folder, &permissions::Holder::Member(ken_id.clone()), Role::Write).unwrap();
        let content = zip_of(&[("Runbook.md", "Restart it.")]);
        import(&core, &ken, ToImport { area_id: area_id.clone(), parent_id: Some(folder), content }).unwrap();
        let listed = &imports(&core, &ken, &area_id)[0];
        assert!(listed.undoable);

        undo_import(&core, &ken, ImportToUndo { import_id: listed.id.clone() }).unwrap();

        assert_eq!(titles(&core, &grace, &area_id), ["Ken's"]);
    }

    #[test]
    fn a_moment_is_dated_in_utc() {
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(1_700_000_000), "2023-11-14");
        assert_eq!(date_of(951_782_400), "2000-02-29");
        assert_eq!(date_of(1_795_651_199), "2026-11-25");
    }
}
