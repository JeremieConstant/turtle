//! The Trash: deleted Pages, Folders and Areas, kept for 30 days so they can
//! be restored.
//!
//! Every Area has a Trash of its own, which those with the Write Role on the
//! Area see: they are the ones who can delete there. What went to it is marked
//! at the top of the deleted Subtree and stays in its place beneath, so it is
//! restored as a whole and to where it was. A deleted Area is kept the same
//! way, but only a Workspace Admin can reach it, since every Role on it is out
//! of reach with it.

use super::accounts::Nothing;
use super::areas::{self, Area, AreaId};
use super::attachments;
use super::permissions::{self, Role};
use super::tree::{self, Entry, Kind};
use super::versions::{self, Author};
use super::{Catalogue, Core, OpError, OpResult, Principal};
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue.add(
        "move_to_trash",
        "Moves a Page or Folder with everything beneath it to the Trash of its Area, where it is kept for 30 days and can be restored. Needs the Write Role.",
        move_to_trash,
    );
    catalogue
        .add(
            "get_trash",
            "Lists the Trash of an Area, newest first: each deleted Page or Folder once, with what was deleted with it, where it was, who deleted it and when, and how many days are left before it is removed for good. Needs the Write Role on the Area.",
            get_trash,
        )
        .changes = false;
    catalogue.add(
        "restore_from_trash",
        "Restores a Page or Folder from the Trash with everything that was deleted with it, to where it was; to the top of the Area when what it was beneath is gone. Needs the Write Role on the Area.",
        restore_from_trash,
    );
    catalogue.add(
        "delete_from_trash",
        "Deletes a Page or Folder in the Trash for good, with everything that was deleted with it. This cannot be undone. Needs the Write Role on the Area.",
        delete_from_trash,
    );
    catalogue.add(
        "delete_area",
        "Moves an Area with everything in it to the Trash, where it is kept for 30 days. Only a Workspace Admin can restore it, except that whoever deleted it can undo that for a few minutes. Needs the Admin Role on it. A Personal Area cannot be deleted.",
        delete_area,
    );
    catalogue
        .add(
            "list_deleted_areas",
            "Lists the deleted Areas, newest first, with who deleted each and when, and how many days are left before it is removed for good. Only a Workspace Admin can.",
            list_deleted_areas,
        )
        .changes = false;
    catalogue.add(
        "restore_area",
        "Restores a deleted Area with everything in it. A Workspace Admin can, and so can whoever deleted it within a few minutes, to undo it.",
        restore_area,
    );
}

/// How long the Trash keeps what is deleted, in days.
pub const KEEP_DAYS: i64 = 30;
const DAY: i64 = 24 * 60 * 60;
/// How long whoever deleted an Area can still undo it, in seconds: long
/// enough for the offer to undo that follows deleting.
const UNDO_AREA_SECONDS: i64 = 5 * 60;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EntryId {
    /// The ID of a Page or Folder.
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TrashOfArea {
    pub area_id: String,
}

/// A Page or Folder that went to the Trash with what was beneath it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Trashed {
    pub id: String,
    /// The Area whose Trash it is in.
    pub area_id: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TrashList {
    pub area: Area,
    /// Newest first.
    pub entries: Vec<TrashEntry>,
}

/// A Page or Folder deleted with everything beneath it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TrashEntry {
    /// Restores with `restore_from_trash`.
    pub id: String,
    pub kind: Kind,
    pub title: String,
    /// The titles of the Folders and Pages it was beneath, from the top of
    /// the Area down; empty when it was at the top.
    pub was_beneath: Vec<String>,
    /// What it was beneath is in the Trash too, so restoring puts it at the
    /// top of the Area.
    pub restores_to_top: bool,
    /// Seconds since the Unix epoch.
    pub deleted_at: i64,
    /// None when they no longer exist.
    pub deleted_by: Option<Author>,
    /// How many Pages and Folders were deleted with it.
    pub entries_beneath: u32,
    /// What was deleted with it, as it was in the tree.
    pub children: Vec<Entry>,
    /// Days before it is removed for good, a day begun counting: 30 just
    /// after deleting, 1 on its last day.
    pub days_left: u32,
}

/// What `restore_from_trash` put back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RestoredEntry {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub area_id: String,
    /// It went to the top of the Area, because what it was beneath is gone.
    pub to_top: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DeletedAreaList {
    /// Newest first.
    pub areas: Vec<DeletedArea>,
}

/// An Area in the Trash with everything in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DeletedArea {
    pub id: String,
    pub name: String,
    /// Seconds since the Unix epoch.
    pub deleted_at: i64,
    /// None when they no longer exist.
    pub deleted_by: Option<Author>,
    /// Days before it is removed for good, a day begun counting: 30 just
    /// after deleting, 1 on its last day.
    pub days_left: u32,
}

pub fn move_to_trash(core: &Core, principal: &Principal, input: EntryId) -> OpResult<Trashed> {
    areas::signed_in(principal)?;
    // Who it is credited to is looked up first: that takes the database too.
    let by = super::tokens::author_of(core, principal)?;
    let db = core.db();
    let not_found = || OpError::NotFound("There is no such Page or Folder.".into());
    let area_id: String = db
        .query_row("SELECT area_id FROM nodes WHERE id = ?1 AND parent_id IS NOT NULL", [&input.id], |row| row.get(0))
        .optional()?
        .ok_or_else(not_found)?;
    let role = permissions::role_on(&db, principal, &input.id)?.ok_or_else(not_found)?;
    if role < Role::Write {
        return Err(OpError::NotPermitted(
            "Only someone with the Write Role can move this to the Trash.".into(),
        ));
    }
    let (member_id, agent_id, agent) = by.stored();
    db.execute(
        "INSERT INTO trash (node_id, trashed_at, member_id, agent_id, agent) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![input.id, core.now(), member_id, agent_id, agent],
    )?;
    let pages = pages_in_subtree(&db, &input.id)?;
    drop(db);
    reach_changed(core, &pages);
    Ok(Trashed { id: input.id, area_id }.into())
}

pub fn get_trash(core: &Core, principal: &Principal, input: TrashOfArea) -> OpResult<TrashList> {
    areas::signed_in(principal)?;
    let db = core.db();
    let area = trash_of(&db, principal, &input.area_id)?;
    let mut statement = db.prepare(
        "SELECT nodes.id, nodes.kind, nodes.title, nodes.parent_id, trash.trashed_at,
                members.id, members.name, trash.agent, agents.id, agents.name, trash.was_beneath
         FROM trash JOIN nodes ON nodes.id = trash.node_id
         LEFT JOIN members ON members.id = trash.member_id
         LEFT JOIN agents ON agents.id = trash.agent_id
         WHERE nodes.area_id = ?1
         ORDER BY trash.trashed_at DESC, trash.rowid DESC",
    )?;
    let rows = statement.query_map([&area.id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            versions::stored_author(row, 5)?,
            row.get::<_, Option<String>>(10)?,
        ))
    })?;
    let rows = rows.collect::<Result<Vec<_>, _>>()?;
    let separately: Vec<String> = rows.iter().map(|row| row.0.clone()).collect();
    let now = core.now();
    let mut entries = Vec::new();
    for (id, kind, title, parent_id, deleted_at, deleted_by, kept) in rows {
        let kind = Kind::from_stored(&kind).ok_or_else(|| anyhow::anyhow!("node {id} has the unknown kind {kind}"))?;
        let children = tree::beneath_in_trash(&db, &area.id, &id, &separately)?;
        // Where it was is kept once that place is gone for good.
        let (was_beneath, restores_to_top) = match kept {
            Some(kept) => (serde_json::from_str(&kept).map_err(anyhow::Error::from)?, true),
            None => (titles_above(&db, &id)?, permissions::in_trash(&db, &parent_id)?),
        };
        entries.push(TrashEntry {
            was_beneath,
            restores_to_top,
            entries_beneath: count(&children),
            children,
            days_left: days_left(deleted_at, now),
            id,
            kind,
            title,
            deleted_at,
            deleted_by,
        });
    }
    Ok(TrashList { area, entries }.into())
}

pub fn restore_from_trash(core: &Core, principal: &Principal, input: EntryId) -> OpResult<RestoredEntry> {
    areas::signed_in(principal)?;
    let db = core.db();
    let found = in_trash_of(&db, principal, &input.id)?;
    // Moved to the top already when what it was beneath was removed for good.
    let moved: bool = db.query_row("SELECT was_beneath IS NOT NULL FROM trash WHERE node_id = ?1", [&input.id], |row| row.get(0))?;
    let parent_in_trash = permissions::in_trash(&db, &found.parent_id)?;
    if parent_in_trash {
        move_to_top(&db, &found.area_id, &input.id)?;
    }
    let to_top = moved || parent_in_trash;
    db.execute("DELETE FROM trash WHERE node_id = ?1", [&input.id])?;
    let pages = pages_in_subtree(&db, &input.id)?;
    drop(db);
    reach_changed(core, &pages);
    Ok(RestoredEntry { id: input.id, kind: found.kind, title: found.title, area_id: found.area_id, to_top }.into())
}

pub fn delete_from_trash(core: &Core, principal: &Principal, input: EntryId) -> OpResult<()> {
    areas::signed_in(principal)?;
    in_trash_of(&core.db(), principal, &input.id)?;
    remove_subtree(core, &input.id)?;
    Ok(().into())
}

pub fn delete_area(core: &Core, principal: &Principal, input: AreaId) -> OpResult<()> {
    areas::signed_in(principal)?;
    let by = super::tokens::author_of(core, principal)?;
    let db = core.db();
    let area = areas::readable_area(&db, principal, &input.id)?;
    if area.personal {
        return Err(OpError::NotPermitted(
            "Your Personal Area cannot be deleted. Move what you no longer need in it to the Trash instead.".into(),
        ));
    }
    if area.role < Role::Admin {
        return Err(OpError::NotPermitted("Only an Admin of this Area can delete it.".into()));
    }
    let (member_id, agent_id, agent) = by.stored();
    db.execute(
        "INSERT INTO deleted_areas (area_id, deleted_at, member_id, agent_id, agent) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![area.id, core.now(), member_id, agent_id, agent],
    )?;
    let pages = pages_in_area(&db, &area.id)?;
    drop(db);
    reach_changed(core, &pages);
    Ok(().into())
}

pub fn list_deleted_areas(core: &Core, principal: &Principal, _: Nothing) -> OpResult<DeletedAreaList> {
    super::require_workspace_admin(principal, "Only a Workspace Admin can see the deleted Areas.")?;
    let db = core.db();
    let mut statement = db.prepare(
        "SELECT areas.id, areas.name, deleted_areas.deleted_at,
                members.id, members.name, deleted_areas.agent, agents.id, agents.name
         FROM deleted_areas JOIN areas ON areas.id = deleted_areas.area_id
         LEFT JOIN members ON members.id = deleted_areas.member_id
         LEFT JOIN agents ON agents.id = deleted_areas.agent_id
         ORDER BY deleted_areas.deleted_at DESC, deleted_areas.rowid DESC",
    )?;
    let now = core.now();
    let areas = statement.query_map([], |row| {
        let deleted_at = row.get(2)?;
        Ok(DeletedArea {
            id: row.get(0)?,
            name: row.get(1)?,
            deleted_at,
            deleted_by: versions::stored_author(row, 3)?,
            days_left: days_left(deleted_at, now),
        })
    })?;
    Ok(DeletedAreaList { areas: areas.collect::<Result<_, _>>()? }.into())
}

pub fn restore_area(core: &Core, principal: &Principal, input: AreaId) -> OpResult<Area> {
    areas::signed_in(principal)?;
    let by = super::tokens::author_of(core, principal)?;
    let db = core.db();
    let deleted: Option<(i64, Option<String>, Option<String>, String)> = db
        .query_row(
            "SELECT deleted_at, member_id, agent_id, agent FROM deleted_areas WHERE area_id = ?1",
            [&input.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    // A deleted Area is out of everyone's reach, so to anyone who may not
    // restore it, it is as if it did not exist.
    let not_found = || OpError::NotFound("There is no such Area.".into());
    let (deleted_at, member_id, agent_id, agent) = deleted.ok_or_else(not_found)?;
    let (by_member, by_agent, by_name) = by.stored();
    let undoing = member_id.as_deref() == by_member
        && agent_id.as_deref() == by_agent
        && agent == by_name
        && core.now() - deleted_at <= UNDO_AREA_SECONDS;
    if !principal.acts_as_workspace_admin() && !undoing {
        return Err(not_found());
    }
    db.execute("DELETE FROM deleted_areas WHERE area_id = ?1", [&input.id])?;
    let pages = pages_in_area(&db, &input.id)?;
    let area = areas::readable_area(&db, principal, &input.id);
    drop(db);
    reach_changed(core, &pages);
    Ok(area?.into())
}

/// Removes for good what has been in the Trash for more than 30 days, with
/// everything in it and its stored content. Returns how many deleted Pages,
/// Folders and Areas went. Turtle runs it once a day.
pub fn remove_expired(core: &Core) -> anyhow::Result<usize> {
    let before = core.now() - KEEP_DAYS * DAY;
    let (areas, entries) = {
        let db = core.db();
        let ids = |sql: &str| -> rusqlite::Result<Vec<String>> {
            let mut statement = db.prepare(sql)?;
            let ids = statement.query_map([before], |row| row.get(0))?;
            ids.collect()
        };
        (
            ids("SELECT area_id FROM deleted_areas WHERE deleted_at < ?1")?,
            ids("SELECT node_id FROM trash WHERE trashed_at < ?1 ORDER BY trashed_at DESC")?,
        )
    };
    let mut removed = 0;
    for area in &areas {
        remove_area(core, area)?;
        removed += 1;
    }
    for entry in &entries {
        // It may have gone with an Area, or with a Subtree removed before it.
        let still_there: bool =
            core.db().query_row("SELECT EXISTS (SELECT 1 FROM trash WHERE node_id = ?1)", [entry], |row| row.get(0))?;
        if still_there {
            remove_subtree(core, entry).map_err(|error| anyhow::anyhow!("removing {entry}: {error:?}"))?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// The number of Pages or Folders that went to the Trash in an Area, for
/// someone who can see its Trash; None for anyone else.
pub(super) fn trash_size(db: &Connection, area: &Area) -> rusqlite::Result<Option<u32>> {
    if area.role < Role::Write {
        return Ok(None);
    }
    db.query_row(
        "SELECT count(*) FROM trash JOIN nodes ON nodes.id = trash.node_id WHERE nodes.area_id = ?1",
        [&area.id],
        |row| row.get(0),
    )
    .map(Some)
}

/// The Area whose Trash the principal may see: with the Write Role on it.
fn trash_of(db: &Connection, principal: &Principal, area_id: &str) -> Result<Area, OpError> {
    let area = areas::readable_area(db, principal, area_id)?;
    if area.role < Role::Write {
        return Err(OpError::NotPermitted(
            "Only someone with the Write Role on this Area can see its Trash.".into(),
        ));
    }
    Ok(area)
}

/// A Page or Folder at the top of what went to the Trash.
struct InTrash {
    area_id: String,
    parent_id: String,
    kind: Kind,
    title: String,
}

/// A Page or Folder that went to the Trash of an Area whose Trash the
/// principal may see. Anything else is not found.
fn in_trash_of(db: &Connection, principal: &Principal, id: &str) -> Result<InTrash, OpError> {
    let not_found = || OpError::NotFound("There is no such Page or Folder in the Trash.".into());
    let found = db
        .query_row(
            "SELECT nodes.area_id, nodes.parent_id, nodes.kind, nodes.title
             FROM trash JOIN nodes ON nodes.id = trash.node_id WHERE trash.node_id = ?1",
            [id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get(3)?)),
        )
        .optional()?;
    let (area_id, parent_id, kind, title) = found.ok_or_else(not_found)?;
    match trash_of(db, principal, &area_id) {
        Ok(_) => {}
        Err(OpError::NotFound(_)) => return Err(not_found()),
        Err(error) => return Err(error),
    }
    let kind = Kind::from_stored(&kind).ok_or_else(|| anyhow::anyhow!("node {id} has the unknown kind {kind}"))?;
    Ok(InTrash { area_id, parent_id, kind, title })
}

/// Removes a Subtree for good with its stored content. What beneath it went
/// to the Trash on its own stays there, now at the top of the Area, since
/// what it was beneath is gone.
fn remove_subtree(core: &Core, node_id: &str) -> Result<(), OpError> {
    let db = core.db();
    let area_id: String = db.query_row("SELECT area_id FROM nodes WHERE id = ?1", [node_id], |row| row.get(0))?;
    let on_their_own: Vec<String> = {
        let mut statement = db.prepare(
            "WITH RECURSIVE downward (id) AS (
                 SELECT id FROM nodes WHERE parent_id = ?1
                 UNION ALL
                 SELECT nodes.id FROM nodes JOIN downward ON nodes.parent_id = downward.id
             )
             SELECT id FROM downward WHERE id IN (SELECT node_id FROM trash)",
        )?;
        let ids = statement.query_map([node_id], |row| row.get(0))?;
        ids.collect::<Result<_, _>>()?
    };
    for id in &on_their_own {
        let was_beneath = serde_json::to_string(&titles_above(&db, id)?).map_err(anyhow::Error::from)?;
        db.execute("UPDATE trash SET was_beneath = ?1 WHERE node_id = ?2", rusqlite::params![was_beneath, id])?;
        move_to_top(&db, &area_id, id)?;
    }
    let pages = pages_in_subtree(&db, node_id)?;
    let files = attachments::files_of(&db, &pages)?;
    forget_content(core, &db, &pages)?;
    // Everything beneath goes with it, with its Versions, Tags, Grants and
    // Attachments.
    db.execute("DELETE FROM nodes WHERE id = ?1", [node_id])?;
    attachments::remove_unattached_files(core, &db, &files)?;
    drop(db);
    for page in &pages {
        core.page_changed(page);
    }
    Ok(())
}

/// Puts an Entry last at the top of its Area.
fn move_to_top(db: &Connection, area_id: &str, id: &str) -> Result<(), OpError> {
    let root = tree::root_of(db, area_id)?;
    db.execute(
        "UPDATE nodes SET parent_id = ?1,
             position = (SELECT COALESCE(MAX(position), 0) + 1 FROM nodes WHERE parent_id = ?1)
         WHERE id = ?2",
        rusqlite::params![root, id],
    )?;
    Ok(())
}

/// Removes a deleted Area for good with everything in it.
fn remove_area(core: &Core, area_id: &str) -> anyhow::Result<()> {
    let db = core.db();
    let pages = pages_in_area(&db, area_id)?;
    let files = attachments::files_of(&db, &pages)?;
    forget_content(core, &db, &pages).map_err(|error| anyhow::anyhow!("removing Area {area_id}: {error:?}"))?;
    db.execute("DELETE FROM areas WHERE id = ?1", [area_id])?;
    attachments::remove_unattached_files(core, &db, &files)?;
    drop(db);
    for page in &pages {
        core.page_changed(page);
    }
    Ok(())
}

/// Removes the stored content of Pages, also from memory, so an open copy
/// is never written back.
fn forget_content(core: &Core, db: &Connection, pages: &[String]) -> Result<(), OpError> {
    let mut statement = db.prepare("DELETE FROM pages WHERE id = ?1")?;
    for page in pages {
        core.live().forget(page);
        statement.execute([page])?;
    }
    Ok(())
}

/// Says that Pages went into or came out of the Trash: search finds them
/// accordingly once it has caught up, and open editors check their access
/// again, so nobody goes on editing a Page in the Trash.
fn reach_changed(core: &Core, pages: &[String]) {
    for page in pages {
        core.page_changed(page);
    }
    core.access_changed();
}

/// Days left of `KEEP_DAYS` since `deleted_at`, a day begun counting as a
/// day left: 30 just after deleting, 1 on the last day.
fn days_left(deleted_at: i64, now: i64) -> u32 {
    let left = deleted_at + KEEP_DAYS * DAY - now;
    ((left.max(0) + DAY - 1) / DAY) as u32
}

fn count(entries: &[Entry]) -> u32 {
    entries.iter().map(|entry| 1 + count(&entry.children)).sum()
}

/// The titles of the Folders and Pages above a node, from the top down,
/// without the Area's hidden root.
fn titles_above(db: &Connection, id: &str) -> rusqlite::Result<Vec<String>> {
    let mut statement = db.prepare(
        "WITH RECURSIVE upward (id, parent_id, title, depth) AS (
             SELECT id, parent_id, title, 0 FROM nodes WHERE id = ?1
             UNION ALL
             SELECT nodes.id, nodes.parent_id, nodes.title, upward.depth + 1
             FROM nodes JOIN upward ON nodes.id = upward.parent_id
         )
         SELECT title FROM upward WHERE depth > 0 AND parent_id IS NOT NULL ORDER BY depth DESC",
    )?;
    let titles = statement.query_map([id], |row| row.get(0))?;
    titles.collect()
}

/// The Pages in a Subtree, its top included, also those that went to the
/// Trash on their own.
fn pages_in_subtree(db: &Connection, node_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut statement = db.prepare(
        "WITH RECURSIVE downward (id, kind) AS (
             SELECT id, kind FROM nodes WHERE id = ?1
             UNION ALL
             SELECT nodes.id, nodes.kind FROM nodes JOIN downward ON nodes.parent_id = downward.id
         )
         SELECT id FROM downward WHERE kind = 'page'",
    )?;
    let pages = statement.query_map([node_id], |row| row.get(0))?;
    pages.collect()
}

fn pages_in_area(db: &Connection, area_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut statement = db.prepare("SELECT id FROM nodes WHERE area_id = ?1 AND kind = 'page'")?;
    let pages = statement.query_map([area_id], |row| row.get(0))?;
    pages.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::areas::{create_area, list_areas, NewArea};
    use crate::ops::content::{get_page_markdown, replace_page, NewContent, OfPage};
    use crate::ops::quick_notes::{create_quick_note, list_quick_notes, NewQuickNote, RecentQuickNotes};
    use crate::ops::search::{search, SearchQuery};
    use crate::ops::tags::{add_tag, list_tags, PageTag};
    use crate::ops::testing::{fresh, give_role, member, personal_area};
    use crate::ops::tree::{create_folder, create_page, get_page, get_tree, NewEntry, PageId, TreeOfArea};

    fn area(core: &Core, admin: &Principal) -> Area {
        create_area(core, admin, NewArea { name: "Engineering".into() }).unwrap().output
    }

    fn entry_in(area_id: &str, parent: Option<&Entry>, title: &str) -> NewEntry {
        NewEntry { area_id: area_id.into(), parent_id: parent.map(|p| p.id.clone()), title: title.into() }
    }

    fn page(core: &Core, writer: &Principal, area: &Area, parent: Option<&Entry>, title: &str) -> Entry {
        create_page(core, writer, entry_in(&area.id, parent, title).into()).unwrap().output
    }

    fn folder(core: &Core, writer: &Principal, area: &Area, parent: Option<&Entry>, title: &str) -> Entry {
        create_folder(core, writer, entry_in(&area.id, parent, title)).unwrap().output
    }

    /// The titles of the tree, children indented by two spaces.
    fn outline(core: &Core, principal: &Principal, area_id: &str) -> Vec<String> {
        let tree = get_tree(core, principal, TreeOfArea { area_id: area_id.into() }).unwrap().output;
        let mut out = Vec::new();
        walk(&tree.entries, 0, &mut out);
        out
    }

    fn walk(entries: &[Entry], depth: usize, out: &mut Vec<String>) {
        for entry in entries {
            out.push(format!("{}{}", "  ".repeat(depth), entry.title));
            walk(&entry.children, depth + 1, out);
        }
    }

    fn trash(core: &Core, principal: &Principal, entry: &Entry) -> Trashed {
        move_to_trash(core, principal, EntryId { id: entry.id.clone() }).unwrap().output
    }

    fn restore(core: &Core, principal: &Principal, entry: &Entry) -> RestoredEntry {
        restore_from_trash(core, principal, EntryId { id: entry.id.clone() }).unwrap().output
    }

    fn listed(core: &Core, principal: &Principal, area_id: &str) -> Vec<TrashEntry> {
        get_trash(core, principal, TrashOfArea { area_id: area_id.into() }).unwrap().output.entries
    }

    fn titles(entries: &[TrashEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.title.as_str()).collect()
    }

    fn not_found_like_missing<T: std::fmt::Debug>(outcome: Result<T, OpError>, missing: Result<T, OpError>) {
        let (outcome, missing) = (outcome.unwrap_err(), missing.unwrap_err());
        assert_eq!((outcome.kind(), outcome.message()), (missing.kind(), missing.message()));
        assert_eq!(outcome.kind(), "not_found");
    }

    #[test]
    fn a_folder_moved_to_the_trash_leaves_the_tree_with_everything_beneath_it() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let guides = folder(&core, &grace, &area, None, "Guides");
        let setup = page(&core, &grace, &area, Some(&guides), "Setup");
        page(&core, &grace, &area, None, "Welcome");

        let trashed = trash(&core, &grace, &guides);

        assert_eq!(trashed, Trashed { id: guides.id.clone(), area_id: area.id.clone() });
        assert_eq!(outline(&core, &grace, &area.id), ["Welcome"]);
        not_found_like_missing(
            get_page(&core, &grace, PageId { id: setup.id.clone() }),
            get_page(&core, &grace, PageId { id: "no-such-page".into() }),
        );
        let beneath_it = create_page(&core, &grace, entry_in(&area.id, Some(&setup), "More").into());
        assert_eq!(beneath_it.unwrap_err().kind(), "not_found");
        let again = move_to_trash(&core, &grace, EntryId { id: setup.id });
        assert_eq!(again.unwrap_err().kind(), "not_found");
    }

    #[test]
    fn moving_to_the_trash_needs_the_write_role() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let notes = page(&core, &grace, &area, None, "Notes");
        let reader = member(&core, "Dennis", false);
        let outsider = member(&core, "Linus", false);
        give_role(&core, &area.id, &reader, Role::Read);

        let refused = move_to_trash(&core, &reader, EntryId { id: notes.id.clone() }).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
        not_found_like_missing(
            move_to_trash(&core, &outsider, EntryId { id: notes.id.clone() }),
            move_to_trash(&core, &outsider, EntryId { id: "no-such-page".into() }),
        );
        let writer = member(&core, "Ken", false);
        give_role(&core, &area.id, &writer, Role::Write);
        trash(&core, &writer, &notes);
        assert!(outline(&core, &grace, &area.id).is_empty());
    }

    #[test]
    fn a_page_in_the_trash_is_found_by_no_search_and_its_tags_are_not_suggested() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let budget = page(&core, &grace, &area, None, "Budget");
        add_tag(&core, &grace, PageTag { page_id: budget.id.clone(), tag: "finance".into() }).unwrap();
        let find = |query: &str| {
            core.catch_up_search().unwrap();
            let input = SearchQuery { query: query.into(), area_id: None, limit: None };
            let results = search(&core, &grace, input).unwrap().output.results;
            results.into_iter().map(|result| result.title).collect::<Vec<_>>()
        };
        assert_eq!(find("budget"), ["Budget"]);

        trash(&core, &grace, &budget);

        assert!(find("budget").is_empty());
        assert!(list_tags(&core, &grace, Nothing {}).unwrap().output.tags.is_empty());
        restore(&core, &grace, &budget);
        assert_eq!(find("budget"), ["Budget"]);
        assert_eq!(list_tags(&core, &grace, Nothing {}).unwrap().output.tags, ["finance"]);
    }

    #[test]
    fn a_quick_note_in_the_trash_is_not_listed_among_the_quick_notes() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let first = create_quick_note(&core, &grace, NewQuickNote { text: "Groceries".into() }).unwrap().output;
        create_quick_note(&core, &grace, NewQuickNote { text: "Ideas".into() }).unwrap();
        let note = Entry { id: first.id, kind: Kind::Page, title: first.title, children: Vec::new() };

        trash(&core, &grace, &note);

        let notes = list_quick_notes(&core, &grace, RecentQuickNotes { limit: None }).unwrap().output.quick_notes;
        let titles: Vec<_> = notes.iter().map(|note| note.title.as_str()).collect();
        assert_eq!(titles, ["Ideas"]);
    }

    #[test]
    fn the_trash_lists_each_deleted_subtree_once_newest_first_with_where_it_was_and_who_deleted_it() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area = area(&core, &grace);
        give_role(&core, &area.id, &ken, Role::Write);
        let guides = folder(&core, &grace, &area, None, "Guides");
        let handbook = page(&core, &grace, &area, Some(&guides), "Handbook");
        page(&core, &grace, &area, Some(&handbook), "Holidays");
        let archive = folder(&core, &grace, &area, Some(&handbook), "Archive");
        page(&core, &grace, &area, Some(&archive), "Old holidays");
        let welcome = page(&core, &grace, &area, None, "Welcome");

        trash(&core, &grace, &handbook);
        core.advance(DAY);
        trash(&core, &ken, &welcome);

        let entries = listed(&core, &grace, &area.id);
        assert_eq!(titles(&entries), ["Welcome", "Handbook"]);
        let [welcome, handbook] = [&entries[0], &entries[1]];
        assert_eq!(welcome.was_beneath, Vec::<String>::new());
        assert_eq!(welcome.deleted_by, Some(Author::Member { id: "member-ken".into(), name: "Ken".into() }));
        assert_eq!(welcome.deleted_at, 1_700_000_000 + DAY);
        assert_eq!((welcome.entries_beneath, welcome.days_left), (0, 30));
        assert_eq!(handbook.kind, Kind::Page);
        assert_eq!(handbook.was_beneath, ["Guides"]);
        assert!(!handbook.restores_to_top);
        assert_eq!(handbook.deleted_by, Some(Author::Member { id: "member-grace".into(), name: "Grace".into() }));
        assert_eq!((handbook.entries_beneath, handbook.days_left), (3, 29));
        let mut beneath = Vec::new();
        walk(&handbook.children, 0, &mut beneath);
        assert_eq!(beneath, ["Holidays", "Archive", "  Old holidays"]);
    }

    #[test]
    fn something_deleted_on_its_own_before_what_it_was_beneath_is_listed_apart_and_restores_to_the_top() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let guides = folder(&core, &grace, &area, None, "Guides");
        let setup = page(&core, &grace, &area, Some(&guides), "Setup");
        page(&core, &grace, &area, Some(&guides), "Deploying");
        page(&core, &grace, &area, None, "Welcome");
        trash(&core, &grace, &setup);
        trash(&core, &grace, &guides);

        let entries = listed(&core, &grace, &area.id);
        assert_eq!(titles(&entries), ["Guides", "Setup"]);
        assert_eq!(entries[0].entries_beneath, 1, "Setup is not counted with Guides");
        assert_eq!((entries[1].was_beneath.clone(), entries[1].restores_to_top), (vec!["Guides".to_string()], true));

        let restored = restore(&core, &grace, &setup);

        assert!(restored.to_top);
        assert_eq!(outline(&core, &grace, &area.id), ["Welcome", "Setup"]);
        assert!(!restore(&core, &grace, &guides).to_top);
        assert_eq!(outline(&core, &grace, &area.id), ["Guides", "  Deploying", "Welcome", "Setup"]);
    }

    #[test]
    fn a_restored_subtree_returns_to_its_place_with_its_content() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let guides = folder(&core, &grace, &area, None, "Guides");
        page(&core, &grace, &area, Some(&guides), "Intro");
        let setup = page(&core, &grace, &area, Some(&guides), "Setup");
        page(&core, &grace, &area, Some(&setup), "Mac");
        page(&core, &grace, &area, Some(&guides), "Deploying");
        replace_page(&core, &grace, NewContent { page_id: setup.id.clone(), markdown: "Install it.".into() }).unwrap();

        trash(&core, &grace, &setup);
        let restored = restore(&core, &grace, &setup);

        assert_eq!(restored, RestoredEntry { id: setup.id.clone(), kind: Kind::Page, title: "Setup".into(), area_id: area.id.clone(), to_top: false });
        assert_eq!(outline(&core, &grace, &area.id), ["Guides", "  Intro", "  Setup", "    Mac", "  Deploying"]);
        let content = get_page_markdown(&core, &grace, OfPage { page_id: setup.id }).unwrap().output;
        assert_eq!(content.markdown.trim(), "Install it.");
        assert!(listed(&core, &grace, &area.id).is_empty());
    }

    #[test]
    fn the_trash_of_an_area_is_seen_and_restored_from_with_the_write_role() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let notes = page(&core, &grace, &area, None, "Notes");
        let writer = member(&core, "Ken", false);
        let reader = member(&core, "Dennis", false);
        let outsider = member(&core, "Linus", false);
        let ada = member(&core, "Ada", true);
        give_role(&core, &area.id, &writer, Role::Write);
        give_role(&core, &area.id, &reader, Role::Read);
        trash(&core, &grace, &notes);

        for seeing in [&grace, &writer, &ada] {
            assert_eq!(titles(&listed(&core, seeing, &area.id)), ["Notes"], "{seeing:?}");
        }
        let refused = get_trash(&core, &reader, TrashOfArea { area_id: area.id.clone() }).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
        let refused = restore_from_trash(&core, &reader, EntryId { id: notes.id.clone() }).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
        let refused = delete_from_trash(&core, &reader, EntryId { id: notes.id.clone() }).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
        not_found_like_missing(
            get_trash(&core, &outsider, TrashOfArea { area_id: area.id.clone() }),
            get_trash(&core, &outsider, TrashOfArea { area_id: "no-such-area".into() }),
        );
        not_found_like_missing(
            restore_from_trash(&core, &outsider, EntryId { id: notes.id.clone() }),
            restore_from_trash(&core, &outsider, EntryId { id: "no-such-page".into() }),
        );
        restore(&core, &writer, &notes);
        assert_eq!(outline(&core, &reader, &area.id), ["Notes"]);
    }

    #[test]
    fn the_tree_says_how_many_subtrees_are_in_the_trash_to_those_who_can_see_it() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let reader = member(&core, "Dennis", false);
        give_role(&core, &area.id, &reader, Role::Read);
        let guides = folder(&core, &grace, &area, None, "Guides");
        page(&core, &grace, &area, Some(&guides), "Setup");
        let notes = page(&core, &grace, &area, None, "Notes");
        let size = |who: &Principal| get_tree(&core, who, TreeOfArea { area_id: area.id.clone() }).unwrap().output.trash;
        assert_eq!(size(&grace), Some(0));

        trash(&core, &grace, &guides);
        trash(&core, &grace, &notes);

        assert_eq!(size(&grace), Some(2));
        assert_eq!(size(&reader), None);
    }

    #[test]
    fn deleting_for_good_removes_the_subtree_but_keeps_what_was_deleted_beneath_it_on_its_own() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let guides = folder(&core, &grace, &area, None, "Guides");
        let setup = page(&core, &grace, &area, Some(&guides), "Setup");
        page(&core, &grace, &area, Some(&guides), "Deploying");
        trash(&core, &grace, &setup);
        trash(&core, &grace, &guides);

        delete_from_trash(&core, &grace, EntryId { id: guides.id.clone() }).unwrap();

        let entries = listed(&core, &grace, &area.id);
        assert_eq!(titles(&entries), ["Setup"]);
        assert_eq!(entries[0].was_beneath, ["Guides"], "it still says where it was");
        assert!(entries[0].restores_to_top);
        let again = restore_from_trash(&core, &grace, EntryId { id: guides.id }).unwrap_err();
        assert_eq!(again.kind(), "not_found");
        assert!(restore(&core, &grace, &setup).to_top);
        assert_eq!(outline(&core, &grace, &area.id), ["Setup"]);
    }

    /// How many Pages still have stored content or Versions. Nothing in the
    /// operations can see content that was removed for good, so the store is
    /// asked directly.
    fn stored_pages(core: &Core) -> (i64, i64) {
        let db = core.db();
        let count = |sql: &str| db.query_row(sql, [], |row| row.get::<_, i64>(0)).unwrap();
        (count("SELECT count(*) FROM pages"), count("SELECT count(DISTINCT page_id) FROM versions"))
    }

    #[test]
    fn what_has_been_in_the_trash_for_more_than_thirty_days_is_removed_with_its_content() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let old = page(&core, &grace, &area, None, "Old");
        let newer = page(&core, &grace, &area, None, "Newer");
        for entry in [&old, &newer] {
            replace_page(&core, &grace, NewContent { page_id: entry.id.clone(), markdown: "Text.".into() }).unwrap();
        }
        core.save_changed_pages().unwrap();
        assert_eq!(stored_pages(&core), (2, 2));
        trash(&core, &grace, &old);
        core.advance(3 * DAY);
        trash(&core, &grace, &newer);

        core.advance(27 * DAY - 1);
        let entries = listed(&core, &grace, &area.id);
        assert_eq!(entries.iter().map(|entry| entry.days_left).collect::<Vec<_>>(), [4, 1], "a day begun counts");
        core.advance(1);
        assert_eq!(remove_expired(&core).unwrap(), 0, "exactly thirty days is not more");

        core.advance(1);
        assert_eq!(remove_expired(&core).unwrap(), 1);
        assert_eq!(titles(&listed(&core, &grace, &area.id)), ["Newer"]);
        assert_eq!(stored_pages(&core), (1, 1));
        core.save_changed_pages().unwrap();
        assert_eq!(stored_pages(&core), (1, 1), "an open copy is not written back");
    }

    #[test]
    fn a_personal_area_cannot_be_deleted_but_has_a_trash_for_its_member_alone() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ada = member(&core, "Ada", true);
        let personal = personal_area(&core, &grace);
        let refused = delete_area(&core, &grace, AreaId { id: personal.clone() }).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
        let notes = create_page(&core, &grace, entry_in(&personal, None, "Diary").into()).unwrap().output;

        trash(&core, &grace, &notes);

        assert_eq!(titles(&listed(&core, &grace, &personal)), ["Diary"]);
        let hidden = get_trash(&core, &ada, TrashOfArea { area_id: personal.clone() }).unwrap_err();
        assert_eq!(hidden.kind(), "not_found");
        restore(&core, &grace, &notes);
        assert_eq!(outline(&core, &grace, &personal), ["Diary"]);
    }

    fn area_names(core: &Core, principal: &Principal) -> Vec<String> {
        let list = list_areas(core, principal, Nothing {}).unwrap().output;
        list.areas.into_iter().filter(|area| !area.personal).map(|area| area.name).collect()
    }

    #[test]
    fn a_deleted_area_leaves_everyone_and_only_a_workspace_admin_lists_and_restores_it() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let ada = member(&core, "Ada", true);
        let area = area(&core, &grace);
        give_role(&core, &area.id, &ken, Role::Read);
        let notes = page(&core, &grace, &area, None, "Notes");
        let refused = delete_area(&core, &ken, AreaId { id: area.id.clone() }).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");

        delete_area(&core, &grace, AreaId { id: area.id.clone() }).unwrap();

        for everyone in [&grace, &ken, &ada] {
            assert!(area_names(&core, everyone).is_empty(), "{everyone:?}");
            let gone = get_page(&core, everyone, PageId { id: notes.id.clone() }).unwrap_err();
            assert_eq!(gone.kind(), "not_found");
        }
        let refused = list_deleted_areas(&core, &grace, Nothing {}).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
        let deleted = list_deleted_areas(&core, &ada, Nothing {}).unwrap().output.areas;
        assert_eq!(
            deleted,
            [DeletedArea {
                id: area.id.clone(),
                name: "Engineering".into(),
                deleted_at: 1_700_000_000,
                deleted_by: Some(Author::Member { id: "member-grace".into(), name: "Grace".into() }),
                days_left: 30,
            }]
        );

        let restored = restore_area(&core, &ada, AreaId { id: area.id.clone() }).unwrap().output;

        assert_eq!(restored.name, "Engineering");
        assert_eq!(area_names(&core, &ken), ["Engineering"]);
        assert_eq!(outline(&core, &grace, &area.id), ["Notes"]);
        assert!(list_deleted_areas(&core, &ada, Nothing {}).unwrap().output.areas.is_empty());
    }

    #[test]
    fn whoever_deleted_an_area_can_undo_it_for_a_few_minutes_and_nobody_else_can() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let barbara = member(&core, "Barbara", false);
        let first = area(&core, &grace);
        give_role(&core, &first.id, &barbara, Role::Admin);
        delete_area(&core, &grace, AreaId { id: first.id.clone() }).unwrap();

        not_found_like_missing(
            restore_area(&core, &barbara, AreaId { id: first.id.clone() }),
            restore_area(&core, &barbara, AreaId { id: "no-such-area".into() }),
        );
        core.advance(UNDO_AREA_SECONDS);
        assert_eq!(restore_area(&core, &grace, AreaId { id: first.id.clone() }).unwrap().output.role, Role::Admin);

        delete_area(&core, &grace, AreaId { id: first.id.clone() }).unwrap();
        core.advance(UNDO_AREA_SECONDS + 1);
        let late = restore_area(&core, &grace, AreaId { id: first.id }).unwrap_err();
        assert_eq!(late.kind(), "not_found");
    }

    #[test]
    fn a_deleted_area_is_removed_for_good_after_thirty_days() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ada = member(&core, "Ada", true);
        let area = area(&core, &grace);
        let notes = page(&core, &grace, &area, None, "Notes");
        let guides = folder(&core, &grace, &area, None, "Guides");
        replace_page(&core, &grace, NewContent { page_id: notes.id.clone(), markdown: "Text.".into() }).unwrap();
        core.save_changed_pages().unwrap();
        trash(&core, &grace, &guides);
        delete_area(&core, &grace, AreaId { id: area.id.clone() }).unwrap();

        core.advance(KEEP_DAYS * DAY + 1);

        assert_eq!(remove_expired(&core).unwrap(), 1);
        assert!(list_deleted_areas(&core, &ada, Nothing {}).unwrap().output.areas.is_empty());
        assert_eq!(stored_pages(&core), (0, 0));
        let gone = restore_area(&core, &ada, AreaId { id: area.id }).unwrap_err();
        assert_eq!(gone.kind(), "not_found");
    }
}
