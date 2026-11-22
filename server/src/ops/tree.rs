//! The tree of an Area: its Pages and Folders, creating and renaming them.

use super::accounts::random_hex;
use super::areas::{self, Area};
use super::permissions::{self, Role};
use super::versions::{self, Author};
use super::{Catalogue, Core, OpError, OpResult, Principal, Reply};
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue.add(
        "create_page",
        "Creates a Page at the end of its parent: the top of an Area, or beneath a Page or Folder, optionally with content in Markdown, which is recorded as its first Version. Needs the Write Role.",
        create_page,
    );
    catalogue.add(
        "create_folder",
        "Creates a Folder, which groups Pages and Folders and has no content, at the end of its parent: the top of an Area, or beneath a Page or Folder. Needs the Write Role.",
        create_folder,
    );
    catalogue.add(
        "rename_page",
        "Renames a Page. Its address keeps working. Needs the Write Role.",
        rename_page,
    );
    catalogue.add(
        "rename_folder",
        "Renames a Folder. Needs the Write Role.",
        rename_folder,
    );
    catalogue
        .add(
            "get_page",
            "Gets a Page by its ID, with its Area, the Folders and Pages it is beneath, when and by whom it was last edited, its Tags, and the caller's Role on it. A Page the caller cannot read is reported as not found.",
            get_page,
        )
        .changes = false;
    catalogue
        .add(
            "get_tree",
            "Gets the tree of Pages and Folders of an Area, each in the order of its siblings, without what is in the Trash, and how many deleted Pages and Folders its Trash holds.",
            get_tree,
        )
        .changes = false;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Page,
    Folder,
}

/// A Page or Folder in the tree of an Area, with what is beneath it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Entry {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub children: Vec<Entry>,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Tree {
    pub area: Area,
    /// The top of the Area, in order.
    pub entries: Vec<Entry>,
    /// How many deleted Pages and Folders are in the Trash of the Area, for
    /// someone with the Write Role, who can see it; None for anyone else.
    pub trash: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewPage {
    #[serde(flatten)]
    pub entry: NewEntry,
    /// What the Page says, in Markdown; empty if absent.
    #[serde(default)]
    pub markdown: Option<String>,
}

impl From<NewEntry> for NewPage {
    /// An empty Page.
    fn from(entry: NewEntry) -> NewPage {
        NewPage { entry, markdown: None }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewEntry {
    pub area_id: String,
    /// The Page or Folder to create it beneath; at the top of the Area if absent.
    #[serde(default)]
    pub parent_id: Option<String>,
    pub title: String,
}

/// A Page as it is opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Page {
    pub id: String,
    pub title: String,
    /// The Area the Page is in.
    pub area: Area,
    /// The Folders and Pages the Page is beneath, from the top of the Area
    /// down. Those the caller cannot read are left out.
    pub path: Vec<PathEntry>,
    /// The Role the caller holds on the Page.
    pub role: Role,
    /// Its newest Version: when it was recorded and whose changes it holds.
    /// None for a Page nobody has written in yet. Changes made since then
    /// count once they are recorded as a Version.
    pub last_edited: Option<LastEdited>,
    /// Its Tags, in alphabetical order.
    pub tags: Vec<String>,
}

/// A Folder or Page above a Page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PathEntry {
    /// Its ID; a Page opens by it with `get_page`.
    pub id: String,
    /// Whether it is a Folder or a Page.
    pub kind: Kind,
    pub title: String,
}

/// When a Page was last edited and by whom, as its newest Version says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct LastEdited {
    /// Seconds since the Unix epoch.
    pub at: i64,
    /// Members, Agents acting for a Member, and standalone Agents, told apart.
    pub by: Vec<Author>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PageId {
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RenameEntry {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TreeOfArea {
    pub area_id: String,
}

pub fn create_page(core: &Core, principal: &Principal, input: NewPage) -> OpResult<Entry> {
    let created = create(core, principal, input.entry, Kind::Page, Placement::Last)?;
    write_first_content(core, principal, created, input.markdown)
}

/// Creates a Page first among its siblings, marked as the given Member's
/// Quick Note, a mark it keeps wherever it is moved.
pub(super) fn create_quick_note_page(
    core: &Core,
    principal: &Principal,
    entry: NewEntry,
    content: Vec<crate::model::Block>,
    member_id: &str,
) -> OpResult<Entry> {
    let created = create(core, principal, entry, Kind::Page, Placement::FirstAsQuickNoteOf(member_id))?;
    if !content.is_empty() {
        super::content::write_content(core, principal, &created.output.id, content)?;
    }
    Ok(created)
}

fn write_first_content(core: &Core, principal: &Principal, created: Reply<Entry>, markdown: Option<String>) -> OpResult<Entry> {
    if let Some(markdown) = markdown {
        let page_id = created.output.id.clone();
        super::content::replace_page(core, principal, super::content::NewContent { page_id, markdown })?;
    }
    Ok(created)
}

/// Where among its siblings a new Page or Folder goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement<'a> {
    Last,
    /// First, as the Quick Note of this Member.
    FirstAsQuickNoteOf(&'a str),
}

pub fn create_folder(core: &Core, principal: &Principal, input: NewEntry) -> OpResult<Entry> {
    create(core, principal, input, Kind::Folder, Placement::Last)
}

fn create(core: &Core, principal: &Principal, input: NewEntry, kind: Kind, placement: Placement<'_>) -> OpResult<Entry> {
    areas::signed_in(principal)?;
    let id = random_hex(16)?;
    let now = core.now();
    let db = core.db();
    let area = areas::readable_area(&db, principal, &input.area_id)?;
    let parent_id = match &input.parent_id {
        None => root_of(&db, &area.id)?,
        Some(parent_id) => {
            let in_area: bool = db.query_row(
                "SELECT EXISTS (SELECT 1 FROM nodes
                                WHERE id = ?1 AND area_id = ?2 AND parent_id IS NOT NULL)",
                rusqlite::params![parent_id, area.id],
                |row| row.get(0),
            )?;
            if !in_area {
                return Err(OpError::NotFound(
                    "There is no such Page or Folder in this Area.".into(),
                ));
            }
            parent_id.clone()
        }
    };
    let role = permissions::role_on(&db, principal, &parent_id)?;
    // A parent in the Trash is out of reach like one that does not exist.
    if role.is_none() {
        return Err(OpError::NotFound("There is no such Page or Folder in this Area.".into()));
    }
    if role < Some(Role::Write) {
        return Err(OpError::NotPermitted(
            "Only someone with the Write Role can add Pages and Folders here.".into(),
        ));
    }
    let title = valid_title(&input.title)?;
    // Positions may go below zero, so putting an Entry first moves no sibling.
    let (position, quick_note_of) = match placement {
        Placement::Last => ("(SELECT COALESCE(MAX(position), 0) + 1 FROM nodes WHERE parent_id = ?3)", None),
        Placement::FirstAsQuickNoteOf(member_id) => {
            ("(SELECT COALESCE(MIN(position), 1) - 1 FROM nodes WHERE parent_id = ?3)", Some(member_id))
        }
    };
    db.execute(
        &format!(
            "INSERT INTO nodes (id, area_id, parent_id, kind, title, position, created_at, quick_note_member_id)
             VALUES (?1, ?2, ?3, ?4, ?5, {position}, ?6, ?7)"
        ),
        rusqlite::params![id, area.id, parent_id, kind.stored(), title, now, quick_note_of],
    )?;
    if kind == Kind::Page {
        core.page_changed(&id);
    }
    Ok(Entry { id, kind, title, children: Vec::new() }.into())
}

pub fn rename_page(core: &Core, principal: &Principal, input: RenameEntry) -> OpResult<Entry> {
    rename(core, principal, input, Kind::Page)
}

pub fn rename_folder(core: &Core, principal: &Principal, input: RenameEntry) -> OpResult<Entry> {
    rename(core, principal, input, Kind::Folder)
}

fn rename(core: &Core, principal: &Principal, input: RenameEntry, kind: Kind) -> OpResult<Entry> {
    areas::signed_in(principal)?;
    let db = core.db();
    // Access is checked before the input, so an unreadable entry stays hidden
    // whatever title is sent.
    let found = readable_entry(&db, principal, &input.id, kind)?;
    if found.role < Role::Write {
        return Err(OpError::NotPermitted(
            "Only someone with the Write Role can rename this.".into(),
        ));
    }
    let title = valid_title(&input.title)?;
    db.execute(
        "UPDATE nodes SET title = ?1 WHERE id = ?2",
        rusqlite::params![title, input.id],
    )?;
    if kind == Kind::Page {
        core.page_changed(&input.id);
    }
    let children = beneath(&db, &found.area_id, &input.id)?;
    Ok(Entry { id: input.id, kind, title, children }.into())
}

pub fn get_page(core: &Core, principal: &Principal, input: PageId) -> OpResult<Page> {
    areas::signed_in(principal)?;
    let db = core.db();
    let found = readable_entry(&db, principal, &input.id, Kind::Page)?;
    let area = areas::readable_area(&db, principal, &found.area_id)?;
    let path = path_to(&db, principal, &input.id)?;
    let last_edited = versions::newest(&db, &input.id)?.map(|newest| LastEdited {
        at: newest.recorded_at,
        by: newest.authors,
    });
    let tags = super::tags::tags_of(&db, &input.id)?;
    Ok(Page {
        id: input.id,
        title: found.title,
        area,
        path,
        role: found.role,
        last_edited,
        tags,
    }
    .into())
}

/// The Folders and Pages above an Entry that the principal can read, from
/// the top of the Area down, without the Area's hidden root.
fn path_to(db: &Connection, principal: &Principal, id: &str) -> Result<Vec<PathEntry>, OpError> {
    let mut statement = db.prepare(
        "WITH RECURSIVE upward (id, parent_id, kind, title, depth) AS (
             SELECT id, parent_id, kind, title, 0 FROM nodes WHERE id = ?1
             UNION ALL
             SELECT nodes.id, nodes.parent_id, nodes.kind, nodes.title, upward.depth + 1
             FROM nodes JOIN upward ON nodes.id = upward.parent_id
         )
         SELECT id, kind, title FROM upward
         WHERE depth > 0 AND parent_id IS NOT NULL
         ORDER BY depth DESC",
    )?;
    let rows = statement.query_map([id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
    })?;
    let mut path = Vec::new();
    for row in rows {
        let (id, kind, title) = row?;
        let kind = Kind::from_stored(&kind)
            .ok_or_else(|| anyhow::anyhow!("node {id} has the unknown kind {kind}"))?;
        // Hidden is not found: an Entry the caller cannot read is not named.
        if permissions::role_on(db, principal, &id)?.is_some() {
            path.push(PathEntry { id, kind, title });
        }
    }
    Ok(path)
}

/// A Page whose content the principal may change: with the Write Role. Not an
/// operation of its own; the editing connection and the Markdown endpoint ask
/// it before they let a change through.
pub fn page_to_change(core: &Core, principal: &Principal, id: &str) -> Result<Page, OpError> {
    let page = get_page(core, principal, PageId { id: id.to_string() })?.output;
    if page.role < Role::Write {
        return Err(OpError::NotPermitted(
            "Only someone with the Write Role can change this Page.".into(),
        ));
    }
    Ok(page)
}

pub fn get_tree(core: &Core, principal: &Principal, input: TreeOfArea) -> OpResult<Tree> {
    areas::signed_in(principal)?;
    let db = core.db();
    let area = areas::readable_area(&db, principal, &input.area_id)?;
    let root_id = root_of(&db, &area.id)?;
    let entries = beneath(&db, &area.id, &root_id)?;
    let trash = super::trash::trash_size(&db, &area)?;
    Ok(Tree { area, entries, trash }.into())
}

/// A Page or Folder the principal can read, with the Role they hold on it.
struct ReadableEntry {
    area_id: String,
    title: String,
    role: Role,
}

/// A Page or Folder of the given kind that the principal can read. One they
/// cannot read is reported exactly like one that does not exist.
fn readable_entry(db: &Connection, principal: &Principal, id: &str, kind: Kind) -> Result<ReadableEntry, OpError> {
    let not_found = || {
        OpError::NotFound(match kind {
            Kind::Page => "There is no such Page.".into(),
            Kind::Folder => "There is no such Folder.".into(),
        })
    };
    let found = db
        .query_row(
            "SELECT area_id, title FROM nodes WHERE id = ?1 AND kind = ?2",
            rusqlite::params![id, kind.stored()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let (area_id, title) = found.ok_or_else(not_found)?;
    let role = permissions::role_on(db, principal, id)?.ok_or_else(not_found)?;
    Ok(ReadableEntry { area_id, title, role })
}

/// Everything beneath a node, each level in the order of its siblings,
/// without what is in the Trash.
pub(super) fn beneath(db: &Connection, area_id: &str, parent_id: &str) -> Result<Vec<Entry>, OpError> {
    beneath_with(db, area_id, parent_id, "AND id NOT IN (SELECT node_id FROM trash)")
}

/// Everything beneath a node, each level in the order of its siblings, also
/// what went to the Trash on its own, which the Trash leaves out itself.
pub(super) fn beneath_in_trash(db: &Connection, area_id: &str, parent_id: &str, separately: &[String]) -> Result<Vec<Entry>, OpError> {
    let mut entries = beneath_with(db, area_id, parent_id, "")?;
    fn prune(entries: &mut Vec<Entry>, separately: &[String]) {
        entries.retain(|entry| !separately.contains(&entry.id));
        for entry in entries {
            prune(&mut entry.children, separately);
        }
    }
    prune(&mut entries, separately);
    Ok(entries)
}

fn beneath_with(db: &Connection, area_id: &str, parent_id: &str, condition: &str) -> Result<Vec<Entry>, OpError> {
    let mut statement = db.prepare(&format!(
        "SELECT id, parent_id, kind, title FROM nodes
         WHERE area_id = ?1 AND parent_id IS NOT NULL {condition}
         ORDER BY position, id"
    ))?;
    let rows = statement.query_map([area_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    // Each parent's children, in order; assembled from the top down below.
    let mut children: HashMap<String, Vec<(String, Kind, String)>> = HashMap::new();
    for row in rows {
        let (id, parent_id, kind, title) = row?;
        let kind = Kind::from_stored(&kind)
            .ok_or_else(|| anyhow::anyhow!("node {id} has the unknown kind {kind}"))?;
        children.entry(parent_id).or_default().push((id, kind, title));
    }
    fn assemble(parent: &str, children: &mut HashMap<String, Vec<(String, Kind, String)>>) -> Vec<Entry> {
        let Some(below) = children.remove(parent) else { return Vec::new() };
        below
            .into_iter()
            .map(|(id, kind, title)| {
                let children = assemble(&id, children);
                Entry { id, kind, title, children }
            })
            .collect()
    }
    Ok(assemble(parent_id, &mut children))
}

impl Kind {
    pub(super) fn stored(self) -> &'static str {
        match self {
            Kind::Page => "page",
            Kind::Folder => "folder",
        }
    }

    pub(super) fn from_stored(stored: &str) -> Option<Kind> {
        match stored {
            "page" => Some(Kind::Page),
            "folder" => Some(Kind::Folder),
            _ => None,
        }
    }
}

/// The hidden root an Area's tree hangs from.
pub(super) fn root_of(db: &Connection, area_id: &str) -> Result<String, OpError> {
    Ok(db.query_row(
        "SELECT id FROM nodes WHERE area_id = ?1 AND parent_id IS NULL",
        [area_id],
        |row| row.get(0),
    )?)
}

/// Long enough for any sensible title, short enough to fit the tree.
pub(super) const MAX_TITLE_LENGTH: usize = 200;

fn valid_title(title: &str) -> Result<String, OpError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(OpError::InvalidInput("Enter a title.".into()));
    }
    if title.chars().count() > MAX_TITLE_LENGTH {
        return Err(OpError::InvalidInput(format!(
            "Choose a title of at most {MAX_TITLE_LENGTH} characters."
        )));
    }
    Ok(title.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::testing::{fresh, give_role, member};

    fn area(core: &Core, admin: &Principal) -> Area {
        create_area(core, admin, NewArea { name: "Engineering".into() }).unwrap().output
    }

    fn at_top(area: &Area, title: &str) -> NewEntry {
        NewEntry { area_id: area.id.clone(), parent_id: None, title: title.into() }
    }

    /// The tree as (kind, title) pairs, children indented by two spaces.
    fn outline(core: &Core, principal: &Principal, area: &Area) -> Vec<String> {
        fn walk(entries: &[Entry], depth: usize, out: &mut Vec<String>) {
            for entry in entries {
                out.push(format!("{}{} {}", "  ".repeat(depth), entry.kind.stored(), entry.title));
                walk(&entry.children, depth + 1, out);
            }
        }
        let tree = get_tree(core, principal, TreeOfArea { area_id: area.id.clone() }).unwrap().output;
        let mut out = Vec::new();
        walk(&tree.entries, 0, &mut out);
        out
    }

    #[test]
    fn a_page_created_at_the_top_appears_in_the_tree() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);

        let page = create_page(&core, &grace, at_top(&area, "Onboarding").into()).unwrap().output;

        assert_eq!(page.kind, Kind::Page);
        assert_eq!(page.title, "Onboarding");
        assert_eq!(outline(&core, &grace, &area), ["page Onboarding"]);
    }

    /// Asserts an outcome: success, or an error of the given kind.
    fn expect<T: std::fmt::Debug>(outcome: Result<T, OpError>, expected: Result<(), &str>, who: &Principal) {
        match expected {
            Ok(()) => assert!(outcome.is_ok(), "{who:?}: {outcome:?}"),
            Err(kind) => assert_eq!(outcome.unwrap_err().kind(), kind, "{who:?}"),
        }
    }

    fn beneath(area: &Area, parent: &Entry, title: &str) -> NewEntry {
        NewEntry { area_id: area.id.clone(), parent_id: Some(parent.id.clone()), title: title.into() }
    }

    #[test]
    fn pages_and_folders_nest_beneath_pages_and_folders_in_the_order_they_were_created() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);

        let guides = create_folder(&core, &grace, at_top(&area, "Guides")).unwrap().output;
        let handbook = create_page(&core, &grace, at_top(&area, "Handbook").into()).unwrap().output;
        create_page(&core, &grace, beneath(&area, &guides, "Setup").into()).unwrap();
        let archive = create_folder(&core, &grace, beneath(&area, &guides, "Archive")).unwrap().output;
        create_page(&core, &grace, beneath(&area, &archive, "Old setup").into()).unwrap();
        create_page(&core, &grace, beneath(&area, &handbook, "Holidays").into()).unwrap();
        create_page(&core, &grace, beneath(&area, &guides, "Deploying").into()).unwrap();
        create_folder(&core, &grace, at_top(&area, "Attic")).unwrap();

        assert_eq!(guides.kind, Kind::Folder);
        assert_eq!(
            outline(&core, &grace, &area),
            [
                "folder Guides",
                "  page Setup",
                "  folder Archive",
                "    page Old setup",
                "  page Deploying",
                "page Handbook",
                "  page Holidays",
                "folder Attic",
            ]
        );
    }

    #[test]
    fn adding_to_the_tree_needs_the_write_role_and_reading_it_the_read_role() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let writer = member(&core, "Ken", false);
        let reader = member(&core, "Dennis", false);
        let outsider = member(&core, "Linus", false);
        give_role(&core, &area.id, &writer, Role::Write);
        give_role(&core, &area.id, &reader, Role::Read);

        let table = [
            (&writer, Ok(())),
            (&reader, Err("not_permitted")),
            (&outsider, Err("not_found")),
            (&Principal::Anonymous, Err("not_permitted")),
        ];
        for (principal, expected) in table {
            expect(create_page(&core, principal, at_top(&area, "Notes").into()), expected, principal);
            expect(create_folder(&core, principal, at_top(&area, "Notes")), expected, principal);
        }

        let seen = ["page Notes", "folder Notes"];
        assert_eq!(outline(&core, &reader, &area), seen);
        let hidden = get_tree(&core, &outsider, TreeOfArea { area_id: area.id.clone() }).unwrap_err();
        assert_eq!(hidden.kind(), "not_found");
        let anonymous = get_tree(&core, &Principal::Anonymous, TreeOfArea { area_id: area.id }).unwrap_err();
        assert_eq!(anonymous.kind(), "not_permitted");
    }

    #[test]
    fn a_parent_must_be_a_page_or_folder_of_the_same_area() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace);
        let sales = area(&core, &grace);
        let elsewhere = create_page(&core, &grace, at_top(&sales, "Pricing").into()).unwrap().output;
        let missing = Entry { id: "no-such-node".into(), ..elsewhere.clone() };

        for parent in [&elsewhere, &missing] {
            let refused = create_page(&core, &grace, beneath(&engineering, parent, "Notes").into()).unwrap_err();
            assert_eq!(refused.kind(), "not_found", "{parent:?}");
        }
        assert!(outline(&core, &grace, &engineering).is_empty());
    }

    #[test]
    fn a_title_must_not_be_blank_or_too_long_and_is_trimmed() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let too_long = "x".repeat(MAX_TITLE_LENGTH + 1);
        for title in ["", "  ", too_long.as_str()] {
            let refused = create_page(&core, &grace, at_top(&area, title).into()).unwrap_err();
            assert_eq!(refused.kind(), "invalid_input", "{title:?}");
        }
        let page = create_page(&core, &grace, at_top(&area, "  Notes ").into()).unwrap().output;
        assert_eq!(page.title, "Notes");
        assert_eq!(outline(&core, &grace, &area), ["page Notes"]);
    }

    fn renamed(entry: &Entry, title: &str) -> RenameEntry {
        RenameEntry { id: entry.id.clone(), title: title.into() }
    }

    #[test]
    fn pages_and_folders_are_renamed_and_siblings_may_share_a_title() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let page = create_page(&core, &grace, at_top(&area, "Notes").into()).unwrap().output;
        let folder = create_folder(&core, &grace, at_top(&area, "Drafts")).unwrap().output;
        create_page(&core, &grace, beneath(&area, &folder, "Idea").into()).unwrap();

        let page = rename_page(&core, &grace, renamed(&page, " Meeting notes ")).unwrap().output;
        let folder = rename_folder(&core, &grace, renamed(&folder, "Meeting notes")).unwrap().output;

        assert_eq!(page.title, "Meeting notes");
        assert_eq!(folder.title, "Meeting notes");
        assert_eq!(folder.children.len(), 1);
        assert_eq!(
            outline(&core, &grace, &area),
            ["page Meeting notes", "folder Meeting notes", "  page Idea"]
        );
    }

    #[test]
    fn renaming_needs_the_write_role_and_the_right_kind() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let page = create_page(&core, &grace, at_top(&area, "Notes").into()).unwrap().output;
        let folder = create_folder(&core, &grace, at_top(&area, "Drafts")).unwrap().output;
        let writer = member(&core, "Ken", false);
        let reader = member(&core, "Dennis", false);
        let outsider = member(&core, "Linus", false);
        give_role(&core, &area.id, &writer, Role::Write);
        give_role(&core, &area.id, &reader, Role::Read);

        let table = [
            (&writer, Ok(())),
            (&reader, Err("not_permitted")),
            (&outsider, Err("not_found")),
            (&Principal::Anonymous, Err("not_permitted")),
        ];
        for (principal, expected) in table {
            expect(rename_page(&core, principal, renamed(&page, "Renamed")), expected, principal);
        }
        let wrong_kind = rename_page(&core, &grace, renamed(&folder, "Renamed")).unwrap_err();
        assert_eq!(wrong_kind.kind(), "not_found");
        let wrong_kind = rename_folder(&core, &grace, renamed(&page, "Renamed")).unwrap_err();
        assert_eq!(wrong_kind.kind(), "not_found");
        let blank = rename_page(&core, &grace, renamed(&page, " ")).unwrap_err();
        assert_eq!(blank.kind(), "invalid_input");
        assert_eq!(outline(&core, &grace, &area), ["page Renamed", "folder Drafts"]);
    }

    #[test]
    fn a_page_is_opened_by_its_id_by_anyone_who_can_read_it() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let folder = create_folder(&core, &grace, at_top(&area, "Guides")).unwrap().output;
        let page = create_page(&core, &grace, beneath(&area, &folder, "Setup").into()).unwrap().output;
        let reader = member(&core, "Dennis", false);
        let outsider = member(&core, "Linus", false);
        give_role(&core, &area.id, &reader, Role::Read);

        let opened = get_page(&core, &reader, PageId { id: page.id.clone() }).unwrap().output;
        assert_eq!(opened.id, page.id);
        assert_eq!(opened.title, "Setup");
        assert_eq!(opened.role, Role::Read);
        assert_eq!(opened.area.id, area.id);
        assert_eq!(opened.area.name, "Engineering");

        let refused = [
            (&outsider, page.id.clone(), "not_found"),
            (&grace, folder.id.clone(), "not_found"),
            (&grace, "no-such-page".to_string(), "not_found"),
            (&Principal::Anonymous, page.id.clone(), "not_permitted"),
        ];
        for (principal, id, kind) in refused {
            let outcome = get_page(&core, principal, PageId { id: id.clone() });
            assert_eq!(outcome.unwrap_err().kind(), kind, "{principal:?} {id}");
        }
    }

    /// The path of a Page as "kind title" pairs.
    fn path_of(core: &Core, principal: &Principal, page: &Entry) -> Vec<String> {
        let opened = get_page(core, principal, PageId { id: page.id.clone() }).unwrap().output;
        opened.path.iter().map(|step| format!("{} {}", step.kind.stored(), step.title)).collect()
    }

    #[test]
    fn a_page_comes_with_the_folders_and_pages_it_is_beneath_from_the_top_down() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let guides = create_folder(&core, &grace, at_top(&area, "Guides")).unwrap().output;
        let handbook = create_page(&core, &grace, beneath(&area, &guides, "Handbook").into()).unwrap().output;
        let setup = create_page(&core, &grace, beneath(&area, &handbook, "Setup").into()).unwrap().output;
        let top = create_page(&core, &grace, at_top(&area, "Welcome").into()).unwrap().output;

        assert_eq!(path_of(&core, &grace, &setup), ["folder Guides", "page Handbook"]);
        assert_eq!(path_of(&core, &grace, &handbook), ["folder Guides"]);
        assert_eq!(path_of(&core, &grace, &top), Vec::<String>::new());
    }

    #[test]
    fn a_page_says_when_it_was_last_edited_and_by_whom() {
        use crate::ops::content::{replace_page, NewContent};
        use crate::ops::versions::Author;
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let empty = create_page(&core, &grace, at_top(&area, "Empty").into()).unwrap().output;
        let notes = create_page(
            &core,
            &grace,
            NewPage { entry: at_top(&area, "Notes"), markdown: Some("First.".into()) },
        )
        .unwrap()
        .output;
        let dennis = member(&core, "Dennis", false);
        give_role(&core, &area.id, &dennis, Role::Write);

        let opened = |id: &str| get_page(&core, &grace, PageId { id: id.into() }).unwrap().output;
        assert_eq!(opened(&empty.id).last_edited, None, "a Page nobody has written in");
        assert_eq!(
            opened(&notes.id).last_edited,
            Some(LastEdited {
                at: 1_700_000_000,
                by: vec![Author::Member { id: "member-grace".into(), name: "Grace".into() }],
            })
        );

        core.advance(300);
        replace_page(&core, &dennis, NewContent { page_id: notes.id.clone(), markdown: "Second.".into() }).unwrap();
        assert_eq!(
            opened(&notes.id).last_edited,
            Some(LastEdited {
                at: 1_700_000_300,
                by: vec![Author::Member { id: "member-dennis".into(), name: "Dennis".into() }],
            })
        );
    }

    #[test]
    fn a_renamed_page_is_still_found_by_the_same_id() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let page = create_page(&core, &grace, at_top(&area, "Notes").into()).unwrap().output;

        rename_page(&core, &grace, renamed(&page, "Minutes")).unwrap();

        let opened = get_page(&core, &grace, PageId { id: page.id }).unwrap().output;
        assert_eq!(opened.title, "Minutes");
    }

    #[test]
    fn changing_a_pages_content_needs_the_write_role() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = area(&core, &grace);
        let page = create_page(&core, &grace, at_top(&area, "Notes").into()).unwrap().output;
        let writer = member(&core, "Ken", false);
        let reader = member(&core, "Dennis", false);
        let outsider = member(&core, "Linus", false);
        give_role(&core, &area.id, &writer, Role::Write);
        give_role(&core, &area.id, &reader, Role::Read);

        let table = [
            (&grace, Ok(Role::Admin)),
            (&writer, Ok(Role::Write)),
            (&reader, Err("not_permitted")),
            (&outsider, Err("not_found")),
            (&Principal::Anonymous, Err("not_permitted")),
        ];
        for (principal, expected) in table {
            let outcome = page_to_change(&core, principal, &page.id);
            match expected {
                Ok(role) => assert_eq!(outcome.unwrap().role, role, "{principal:?}"),
                Err(kind) => assert_eq!(outcome.unwrap_err().kind(), kind, "{principal:?}"),
            }
        }
    }
}
