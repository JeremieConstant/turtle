//! Moving Pages and Folders: reordering them among their siblings, putting
//! them beneath another Entry or at the top of an Area, also of another Area.

use super::areas::{self, Area};
use super::permissions::{self, Role};
use super::tree::{self, Kind, PathEntry};
use super::{Catalogue, Core, OpError, OpResult, Principal};
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue.add(
        "move_entry",
        "Moves a Page or Folder with everything beneath it to a place: beneath a Page or Folder or at the top of an Area, before a given sibling or last. The Area may be another one. Its address and its open editors keep working, and access to it follows the Area it is in. Needs the Write Role on it and on the place it goes to. Returns where it was, which moves it back.",
        move_entry,
    );
}

/// A place in the tree of an Area.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Place {
    pub area_id: String,
    /// The Page or Folder it is beneath; the top of the Area if absent.
    #[serde(default)]
    pub parent_id: Option<String>,
    /// The sibling it comes before; last among its siblings if absent.
    #[serde(default)]
    pub before_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveEntry {
    /// The Page or Folder to move.
    pub id: String,
    #[serde(flatten)]
    pub to: Place,
}

/// A Page or Folder where it was moved to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct MovedEntry {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    /// The Area it is in now.
    pub area: Area,
    /// The Folders and Pages it is beneath now, from the top of the Area down.
    pub path: Vec<PathEntry>,
    /// Where it was: moving it there puts it back.
    pub was: Place,
}

pub fn move_entry(core: &Core, principal: &Principal, input: MoveEntry) -> OpResult<MovedEntry> {
    areas::signed_in(principal)?;
    let mut db = core.db();
    // A move is several changes; a failure halfway must not leave a Subtree
    // split between two Areas.
    let db = db.transaction()?;
    // Access is checked before the place, so an unreadable Entry stays hidden
    // wherever it is sent.
    let moving = tree::readable_entry(&db, principal, &input.id, None)?;
    if moving.role < Role::Write {
        return Err(OpError::NotPermitted("Only someone with the Write Role can move this.".into()));
    }
    let area = areas::readable_area(&db, principal, &input.to.area_id)?;
    let not_found = || OpError::NotFound("There is no such Page or Folder in this Area.".into());
    let parent_id = match &input.to.parent_id {
        None => tree::root_of(&db, &area.id)?,
        Some(parent_id) => {
            let in_area: bool = db.query_row(
                "SELECT EXISTS (SELECT 1 FROM nodes WHERE id = ?1 AND area_id = ?2 AND parent_id IS NOT NULL)",
                rusqlite::params![parent_id, area.id],
                |row| row.get(0),
            )?;
            if !in_area {
                return Err(not_found());
            }
            parent_id.clone()
        }
    };
    // A place in the Trash is out of reach like one that does not exist.
    let role = permissions::role_on(&db, principal, &parent_id)?.ok_or_else(not_found)?;
    if role < Role::Write {
        return Err(OpError::NotPermitted(
            "Only someone with the Write Role there can move Pages and Folders to this place.".into(),
        ));
    }
    if beneath_or_at(&db, &parent_id, &input.id)? || input.to.before_id.as_deref() == Some(input.id.as_str()) {
        return Err(OpError::InvalidInput(
            "A Page or Folder cannot be moved beneath itself or anything beneath it. Choose another place.".into(),
        ));
    }
    if let Some(before_id) = &input.to.before_id {
        // A sibling in the Trash, or one the caller cannot read, is no place.
        if permissions::role_on(&db, principal, before_id)?.is_none() {
            return Err(no_sibling());
        }
    }
    let was = place_of(&db, &input.id)?;
    let position = position_before(&db, &parent_id, &input.id, input.to.before_id.as_deref())?;
    db.execute(
        "UPDATE nodes SET parent_id = ?1, position = ?2 WHERE id = ?3",
        rusqlite::params![parent_id, position, input.id],
    )?;
    let nodes = in_subtree(&db, &input.id)?;
    let other_area = moving.area_id != area.id;
    if other_area {
        // What went to the Trash on its own beneath it goes along, into the
        // Trash of the Area it is in now.
        let mut statement = db.prepare("UPDATE nodes SET area_id = ?1 WHERE id = ?2")?;
        for (node, _) in &nodes {
            statement.execute([&area.id, node])?;
        }
    }
    let path = tree::path_to(&db, principal, &input.id)?;
    db.commit()?;
    // Search keeps each Page's Area and place; access follows the new Area,
    // so open editors check theirs again.
    for (node, kind) in &nodes {
        if *kind == Kind::Page {
            core.page_changed(node);
        }
    }
    if other_area {
        core.access_changed();
    }
    Ok(MovedEntry { id: input.id, kind: moving.kind, title: moving.title, area, path, was }.into())
}

fn no_sibling() -> OpError {
    OpError::NotFound("There is no such Page or Folder to put it before in this place.".into())
}

/// Whether `node` is `top` or beneath it.
fn beneath_or_at(db: &Connection, node: &str, top: &str) -> rusqlite::Result<bool> {
    db.query_row(
        "WITH RECURSIVE upward (id, parent_id) AS (
             SELECT id, parent_id FROM nodes WHERE id = ?1
             UNION ALL
             SELECT nodes.id, nodes.parent_id FROM nodes JOIN upward ON nodes.id = upward.parent_id
         )
         SELECT EXISTS (SELECT 1 FROM upward WHERE id = ?2)",
        [node, top],
        |row| row.get(0),
    )
}

/// Every node in a Subtree, its top included, also what went to the Trash
/// on its own, with its kind.
fn in_subtree(db: &Connection, top: &str) -> Result<Vec<(String, Kind)>, OpError> {
    let mut statement = db.prepare(
        "WITH RECURSIVE downward (id, kind) AS (
             SELECT id, kind FROM nodes WHERE id = ?1
             UNION ALL
             SELECT nodes.id, nodes.kind FROM nodes JOIN downward ON nodes.parent_id = downward.id
         )
         SELECT id, kind FROM downward",
    )?;
    let rows = statement.query_map([top], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    let mut nodes = Vec::new();
    for row in rows {
        let (id, kind) = row?;
        let kind = Kind::from_stored(&kind).ok_or_else(|| anyhow::anyhow!("node {id} has the unknown kind {kind}"))?;
        nodes.push((id, kind));
    }
    Ok(nodes)
}

/// Where an Entry is now: its Area, its parent, and the sibling after it
/// that is not in the Trash.
fn place_of(db: &Connection, id: &str) -> Result<Place, OpError> {
    let (area_id, parent_id, parent_is_root, position): (String, String, bool, f64) = db.query_row(
        "SELECT nodes.area_id, nodes.parent_id, parent.parent_id IS NULL, nodes.position
         FROM nodes JOIN nodes AS parent ON parent.id = nodes.parent_id WHERE nodes.id = ?1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let before_id = db
        .query_row(
            "SELECT id FROM nodes
             WHERE parent_id = ?1 AND id <> ?2 AND id NOT IN (SELECT node_id FROM trash)
               AND (position > ?3 OR (position = ?3 AND id > ?2))
             ORDER BY position, id LIMIT 1",
            rusqlite::params![parent_id, id, position],
            |row| row.get(0),
        )
        .optional()?;
    Ok(Place { area_id, parent_id: (!parent_is_root).then_some(parent_id), before_id })
}

/// The position that puts an Entry before a sibling, or last, beneath a
/// parent. Positions are numbers with room between them, so only the moved
/// Entry's changes; once two neighbours are too close to fit another number
/// between them, the siblings are numbered afresh. The column is declared
/// INTEGER, which SQLite lets hold the fractions too. What is in the Trash
/// keeps its position, and sorts among its siblings again once restored.
fn position_before(db: &Connection, parent_id: &str, id: &str, before_id: Option<&str>) -> Result<f64, OpError> {
    let siblings: Vec<(String, f64)> = {
        let mut statement = db.prepare(
            "SELECT id, position FROM nodes
             WHERE parent_id = ?1 AND id <> ?2 AND id NOT IN (SELECT node_id FROM trash)
             ORDER BY position, id",
        )?;
        let rows = statement.query_map([parent_id, id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let Some(before_id) = before_id else {
        return Ok(siblings.last().map_or(1.0, |(_, last)| (last + 1.0).floor()));
    };
    let index = siblings.iter().position(|(sibling, _)| sibling == before_id).ok_or_else(no_sibling)?;
    let next = siblings[index].1;
    let Some((_, previous)) = index.checked_sub(1).map(|previous| &siblings[previous]) else {
        return Ok((next - 1.0).floor());
    };
    let between = previous + (next - previous) / 2.0;
    if *previous < between && between < next {
        return Ok(between);
    }
    let mut renumber = db.prepare("UPDATE nodes SET position = ?1 WHERE id = ?2")?;
    for (number, (sibling, _)) in siblings.iter().enumerate() {
        let number = if number < index { number } else { number + 1 };
        renumber.execute(rusqlite::params![number as f64, sibling])?;
    }
    Ok(index as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::testing::{fresh, give_role, member, personal_area, take_role};
    use crate::ops::tree::{create_folder, create_page, get_tree, Entry, NewEntry, TreeOfArea};

    fn area(core: &Core, admin: &Principal, name: &str) -> Area {
        create_area(core, admin, NewArea { name: name.into() }).unwrap().output
    }

    fn page(core: &Core, writer: &Principal, area: &Area, parent: Option<&Entry>, title: &str) -> Entry {
        let entry = NewEntry { area_id: area.id.clone(), parent_id: parent.map(|p| p.id.clone()), title: title.into() };
        create_page(core, writer, entry.into()).unwrap().output
    }

    fn folder(core: &Core, writer: &Principal, area: &Area, parent: Option<&Entry>, title: &str) -> Entry {
        let entry = NewEntry { area_id: area.id.clone(), parent_id: parent.map(|p| p.id.clone()), title: title.into() };
        create_folder(core, writer, entry).unwrap().output
    }

    /// The tree as titles, children indented by two spaces.
    fn outline(core: &Core, principal: &Principal, area: &Area) -> Vec<String> {
        fn walk(entries: &[Entry], depth: usize, out: &mut Vec<String>) {
            for entry in entries {
                out.push(format!("{}{}", "  ".repeat(depth), entry.title));
                walk(&entry.children, depth + 1, out);
            }
        }
        let tree = get_tree(core, principal, TreeOfArea { area_id: area.id.clone() }).unwrap().output;
        let mut out = Vec::new();
        walk(&tree.entries, 0, &mut out);
        out
    }

    fn to(area: &Area, parent: Option<&Entry>, before: Option<&Entry>) -> Place {
        Place { area_id: area.id.clone(), parent_id: parent.map(|p| p.id.clone()), before_id: before.map(|b| b.id.clone()) }
    }

    fn moving(core: &Core, principal: &Principal, entry: &Entry, place: Place) -> OpResult<MovedEntry> {
        move_entry(core, principal, MoveEntry { id: entry.id.clone(), to: place })
    }

    #[test]
    fn an_entry_moves_before_a_sibling_or_last_and_everyone_sees_the_new_order() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace, "Engineering");
        let a = page(&core, &grace, &area, None, "A");
        let b = page(&core, &grace, &area, None, "B");
        let c = page(&core, &grace, &area, None, "C");

        moving(&core, &grace, &c, to(&area, None, Some(&b))).unwrap();
        assert_eq!(outline(&core, &grace, &area), ["A", "C", "B"]);
        moving(&core, &grace, &a, to(&area, None, None)).unwrap();
        assert_eq!(outline(&core, &grace, &area), ["C", "B", "A"]);
        moving(&core, &grace, &a, to(&area, None, Some(&c))).unwrap();
        assert_eq!(outline(&core, &grace, &area), ["A", "C", "B"]);
        // Again and again between the same two, each time one position changes.
        for _ in 0..80 {
            moving(&core, &grace, &b, to(&area, None, Some(&c))).unwrap();
            moving(&core, &grace, &c, to(&area, None, Some(&b))).unwrap();
            assert_eq!(outline(&core, &grace, &area), ["A", "C", "B"]);
        }
    }

    #[test]
    fn a_subtree_moves_beneath_another_entry_and_back_to_the_top() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace, "Engineering");
        let guides = folder(&core, &grace, &area, None, "Guides");
        let setup = page(&core, &grace, &area, None, "Setup");
        page(&core, &grace, &area, Some(&setup), "Laptop");
        let handbook = page(&core, &grace, &area, Some(&guides), "Handbook");

        let moved = moving(&core, &grace, &setup, to(&area, Some(&guides), Some(&handbook))).unwrap().output;
        assert_eq!(outline(&core, &grace, &area), ["Guides", "  Setup", "    Laptop", "  Handbook"]);
        assert_eq!(moved.path, [PathEntry { id: guides.id.clone(), kind: Kind::Folder, title: "Guides".into() }]);
        assert_eq!(moved.was, to(&area, None, None));

        let moved = moving(&core, &grace, &setup, to(&area, None, Some(&guides))).unwrap().output;
        assert_eq!(outline(&core, &grace, &area), ["Setup", "  Laptop", "Guides", "  Handbook"]);
        assert_eq!(moved.path, []);
        assert_eq!(moved.was, to(&area, Some(&guides), Some(&handbook)));
    }

    #[test]
    fn an_entry_cannot_move_beneath_itself_or_what_is_beneath_it() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace, "Engineering");
        let guides = folder(&core, &grace, &area, None, "Guides");
        let setup = page(&core, &grace, &area, Some(&guides), "Setup");
        let laptop = page(&core, &grace, &area, Some(&setup), "Laptop");

        for parent in [&guides, &setup, &laptop] {
            let refused = moving(&core, &grace, &guides, to(&area, Some(parent), None)).unwrap_err();
            assert_eq!(refused.kind(), "invalid_input", "{}", parent.title);
        }
        let refused = moving(&core, &grace, &setup, to(&area, Some(&guides), Some(&setup))).unwrap_err();
        assert_eq!(refused.kind(), "invalid_input");
        assert_eq!(outline(&core, &grace, &area), ["Guides", "  Setup", "    Laptop"]);
    }

    #[test]
    fn a_subtree_moves_to_another_area_where_access_follows_that_area_and_its_address_keeps_working() {
        use crate::ops::tree::{get_page, PageId};
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let sales = area(&core, &grace, "Sales");
        let guides = folder(&core, &grace, &engineering, None, "Guides");
        let setup = page(&core, &grace, &engineering, Some(&guides), "Setup");
        let pricing = page(&core, &grace, &sales, None, "Pricing");
        let engineer = member(&core, "Ken", false);
        let seller = member(&core, "Dennis", false);
        give_role(&core, &engineering.id, &engineer, Role::Read);
        give_role(&core, &sales.id, &seller, Role::Read);

        let moved = moving(&core, &grace, &guides, to(&sales, None, Some(&pricing))).unwrap().output;
        assert_eq!(moved.area.id, sales.id);
        assert_eq!(moved.was, to(&engineering, None, None));
        assert_eq!(outline(&core, &grace, &sales), ["Guides", "  Setup", "Pricing"]);
        assert!(outline(&core, &grace, &engineering).is_empty());

        let opened = |who: &Principal| get_page(&core, who, PageId { id: setup.id.clone() });
        assert_eq!(opened(&grace).unwrap().output.area.id, sales.id);
        assert_eq!(opened(&seller).unwrap().output.title, "Setup");
        assert_eq!(opened(&engineer).unwrap_err().kind(), "not_found");

        let back = moved.was;
        moving(&core, &grace, &guides, back).unwrap();
        assert_eq!(outline(&core, &grace, &engineering), ["Guides", "  Setup"]);
        assert_eq!(opened(&engineer).unwrap().output.title, "Setup");
        assert_eq!(opened(&seller).unwrap_err().kind(), "not_found");
    }

    #[test]
    fn moving_needs_the_write_role_on_the_entry_and_on_the_place_it_goes_to() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let sales = area(&core, &grace, "Sales");
        let hidden = area(&core, &grace, "Board");
        let notes = page(&core, &grace, &engineering, None, "Notes");
        let target = folder(&core, &grace, &sales, None, "Inbox");
        let ken = member(&core, "Ken", false);

        let table = [
            (Some(Role::Write), Some(Role::Write), Ok(())),
            (Some(Role::Write), Some(Role::Read), Err("not_permitted")),
            (Some(Role::Read), Some(Role::Write), Err("not_permitted")),
            (None, Some(Role::Write), Err("not_found")),
            (Some(Role::Write), None, Err("not_found")),
        ];
        for (on_entry, on_place, expected) in table {
            for (area, role) in [(&engineering, on_entry), (&sales, on_place)] {
                match role {
                    Some(role) => give_role(&core, &area.id, &ken, role),
                    None => take_role(&core, &area.id, &ken),
                }
            }
            let outcome = moving(&core, &ken, &notes, to(&sales, Some(&target), None));
            match expected {
                Ok(()) => {
                    let moved = outcome.unwrap().output;
                    moving(&core, &grace, &notes, moved.was).unwrap();
                }
                Err(kind) => assert_eq!(outcome.unwrap_err().kind(), kind, "{on_entry:?} {on_place:?}"),
            }
        }
        // A parent from another Area than the one named is not there.
        let refused = moving(&core, &grace, &notes, to(&hidden, Some(&target), None)).unwrap_err();
        assert_eq!(refused.kind(), "not_found");
        let anonymous = moving(&core, &Principal::Anonymous, &notes, to(&sales, None, None)).unwrap_err();
        assert_eq!(anonymous.kind(), "not_permitted");
        assert_eq!(outline(&core, &grace, &engineering), ["Notes"]);
    }

    #[test]
    fn nothing_moves_into_another_members_personal_area() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let notes = page(&core, &grace, &engineering, None, "Notes");
        let ken = member(&core, "Ken", false);
        let kens = Area { id: personal_area(&core, &ken), name: String::new(), role: Role::Admin, personal: true };

        let refused = moving(&core, &grace, &notes, to(&kens, None, None)).unwrap_err();
        assert_eq!(refused.kind(), "not_found");
    }

    #[test]
    fn a_moved_page_is_found_by_search_in_its_new_area() {
        use crate::ops::search::{search, SearchQuery};
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let sales = area(&core, &grace, "Sales");
        let roadmap = page(&core, &grace, &engineering, None, "Roadmap");

        moving(&core, &grace, &roadmap, to(&sales, None, None)).unwrap();

        core.catch_up_search().unwrap();
        let found = search(&core, &grace, SearchQuery { query: "roadmap".into(), area_id: Some(sales.id.clone()), limit: None })
            .unwrap()
            .output;
        assert_eq!(found.results.len(), 1);
        assert_eq!(found.results[0].area_id, sales.id);
    }

    #[test]
    fn what_is_in_the_trash_is_no_place_to_move_to_but_goes_along_beneath_what_moves() {
        use crate::ops::trash::{get_trash, move_to_trash, EntryId, TrashOfArea};
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let sales = area(&core, &grace, "Sales");
        let guides = folder(&core, &grace, &engineering, None, "Guides");
        let old = page(&core, &grace, &engineering, Some(&guides), "Old");
        let notes = page(&core, &grace, &engineering, None, "Notes");
        let attic = folder(&core, &grace, &engineering, None, "Attic");
        move_to_trash(&core, &grace, EntryId { id: old.id.clone() }).unwrap();
        move_to_trash(&core, &grace, EntryId { id: attic.id.clone() }).unwrap();

        let refused = moving(&core, &grace, &notes, to(&engineering, None, Some(&attic))).unwrap_err();
        assert_eq!(refused.kind(), "not_found", "before what is in the Trash");
        let refused = moving(&core, &grace, &notes, to(&engineering, Some(&attic), None)).unwrap_err();
        assert_eq!(refused.kind(), "not_found", "beneath what is in the Trash");

        moving(&core, &grace, &guides, to(&sales, None, None)).unwrap();
        let in_trash = |area: &Area| -> Vec<String> {
            let trash = get_trash(&core, &grace, TrashOfArea { area_id: area.id.clone() }).unwrap().output;
            trash.entries.into_iter().map(|entry| entry.title).collect()
        };
        assert_eq!(in_trash(&sales), ["Old"]);
        assert_eq!(in_trash(&engineering), ["Attic"]);
    }
}
