//! Versions: recorded states of a Page, listed, viewed, compared and restored.
//! When they are recorded is decided in `live`, where the open Pages are.

use crate::diff::{align, Fit, Step};
use super::content::change_page;
use super::tree::{get_page, page_to_change, Page, PageId};
use super::{Catalogue, Core, OpError, OpResult, Principal};
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue.add(
        "list_versions",
        "Lists the Versions of a Page, newest first, each with when it was recorded and who wrote the changes it holds. Needs the Read Role.",
        list_versions,
    );
    catalogue.add(
        "get_version",
        "Gets one Version of a Page with its content as Markdown. Needs the Read Role.",
        get_version,
    );
    catalogue.add(
        "compare_versions",
        "Compares two Versions of a Page line by line in Markdown, showing what was added and removed going from one to the other. Needs the Read Role.",
        compare_versions,
    );
    catalogue.add(
        "restore_version",
        "Makes a Page say what one of its Versions said, as a new Version; the Versions in between stay. Only what differs is changed, so people editing at that moment keep their work and their cursor. Needs the Write Role.",
        restore_version,
    );
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VersionsOfPage {
    /// The Page whose Versions to list.
    pub page_id: String,
}

/// Someone whose changes a Version holds: a Member, or an Agent acting
/// through one of their Tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Author {
    /// The Member, or the Member whose Token the Agent acted through.
    pub id: String,
    /// The Member's name.
    pub name: String,
    /// The Agent's name, which is the name of the Token it acted through.
    /// Absent when the Member made the changes themself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

/// A Version as it is listed, without its content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct VersionSummary {
    /// Counts up from 1 within the Page.
    pub number: u32,
    /// Seconds since the Unix epoch.
    pub recorded_at: i64,
    /// Whose changes arrived since the previous Version, by name.
    pub authors: Vec<Author>,
    /// The number of the Version this one restored, if it was a restore.
    pub restored_from: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VersionOfPage {
    /// The Page the Version belongs to.
    pub page_id: String,
    /// The Version's number within the Page.
    pub number: u32,
}

/// A Version with its content.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Version {
    #[serde(flatten)]
    pub summary: VersionSummary,
    /// The Page as it was, in Markdown.
    pub markdown: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TwoVersions {
    /// The Page both Versions belong to.
    pub page_id: String,
    /// The number of the Version to compare from. Lines only it has count as removed.
    pub from: u32,
    /// The number of the Version to compare to. Lines only it has count as added.
    pub to: u32,
}

/// What became of a line between the two Versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    /// In both.
    Same,
    /// Only in `to`.
    Added,
    /// Only in `from`.
    Removed,
}

/// One line of the Markdown of either Version.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ComparedLine {
    pub change: Change,
    pub text: String,
}

/// The lines of both Versions in order, each marked with what became of it.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Comparison {
    pub from: VersionSummary,
    pub to: VersionSummary,
    pub lines: Vec<ComparedLine>,
}

/// What a restore recorded.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Restored {
    /// The new Version, which says what the restored one said. None when the
    /// Page already said that and its last Version did too.
    pub version: Option<VersionSummary>,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct VersionList {
    pub versions: Vec<VersionSummary>,
}

pub fn list_versions(core: &Core, principal: &Principal, input: VersionsOfPage) -> OpResult<VersionList> {
    readable_page(core, principal, &input.page_id)?;
    let db = core.db();
    let mut statement = db.prepare(
        "SELECT number, recorded_at, restored_from FROM versions WHERE page_id = ?1 ORDER BY number DESC",
    )?;
    let rows = statement.query_map([&input.page_id], |row| {
        Ok((row.get::<_, u32>(0)?, row.get::<_, i64>(1)?, row.get::<_, Option<u32>>(2)?))
    })?;
    let mut versions = Vec::new();
    for row in rows {
        let (number, recorded_at, restored_from) = row?;
        let authors = authors_of(&db, &input.page_id, number)?;
        versions.push(VersionSummary { number, recorded_at, authors, restored_from });
    }
    Ok(VersionList { versions }.into())
}

pub fn get_version(core: &Core, principal: &Principal, input: VersionOfPage) -> OpResult<Version> {
    readable_page(core, principal, &input.page_id)?;
    Ok(version(&core.db(), &input.page_id, input.number)?.into())
}

pub fn compare_versions(core: &Core, principal: &Principal, input: TwoVersions) -> OpResult<Comparison> {
    readable_page(core, principal, &input.page_id)?;
    let db = core.db();
    let from = version(&db, &input.page_id, input.from)?;
    let to = version(&db, &input.page_id, input.to)?;
    let lines = compare_lines(&from.markdown, &to.markdown);
    Ok(Comparison { from: from.summary, to: to.summary, lines }.into())
}

/// The lines of both texts, paired up where they are the same.
fn compare_lines(from: &str, to: &str) -> Vec<ComparedLine> {
    let old: Vec<&str> = from.lines().collect();
    let new: Vec<&str> = to.lines().collect();
    let fit = |i: usize, j: usize| if old[i] == new[j] { Fit::Same(1) } else { Fit::No };
    let mut steps = align(old.len(), new.len(), fit, |_| false);
    // Between two kept lines, what was removed reads before what replaced it.
    for run in steps.split_mut(|step| matches!(step, Step::Keep(..))) {
        run.sort_by_key(|step| matches!(step, Step::Add(_)));
    }
    steps
        .into_iter()
        .map(|step| {
            let (change, text) = match step {
                Step::Keep(i, _) => (Change::Same, old[i]),
                Step::Remove(i) => (Change::Removed, old[i]),
                Step::Add(j) => (Change::Added, new[j]),
            };
            ComparedLine { change, text: text.to_string() }
        })
        .collect()
}

pub fn restore_version(core: &Core, principal: &Principal, input: VersionOfPage) -> OpResult<Restored> {
    page_to_change(core, principal, &input.page_id)?;
    let restoring = version(&core.db(), &input.page_id, input.number)?;
    let blocks = crate::markdown::from_markdown(&restoring.markdown);
    let version = change_page(core, principal, &input.page_id, Some(input.number), |_| Ok(blocks))?;
    Ok(Restored { version }.into())
}

/// A Version of a Page the caller may read.
pub(super) fn version(db: &Connection, page_id: &str, number: u32) -> Result<Version, OpError> {
    let found = db
        .query_row(
            "SELECT recorded_at, restored_from, markdown FROM versions WHERE page_id = ?1 AND number = ?2",
            rusqlite::params![page_id, number],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<u32>>(1)?, row.get::<_, String>(2)?)),
        )
        .optional()?;
    let (recorded_at, restored_from, markdown) =
        found.ok_or_else(|| OpError::NotFound("There is no such Version.".into()))?;
    let authors = authors_of(db, page_id, number)?;
    Ok(Version { summary: VersionSummary { number, recorded_at, authors, restored_from }, markdown })
}

/// Checks that the principal can read the Page; one they cannot read is not found.
fn readable_page(core: &Core, principal: &Principal, page_id: &str) -> Result<Page, OpError> {
    Ok(get_page(core, principal, PageId { id: page_id.to_string() })?.output)
}

fn authors_of(db: &Connection, page_id: &str, number: u32) -> rusqlite::Result<Vec<Author>> {
    let mut statement = db.prepare(
        "SELECT members.id, members.name, version_authors.agent FROM version_authors
         JOIN members ON members.id = version_authors.member_id
         WHERE version_authors.page_id = ?1 AND version_authors.number = ?2
         ORDER BY members.name COLLATE NOCASE, members.id, version_authors.agent",
    )?;
    let authors = statement.query_map(rusqlite::params![page_id, number], |row| {
        let agent: String = row.get(2)?;
        Ok(Author { id: row.get(0)?, name: row.get(1)?, agent: (!agent.is_empty()).then_some(agent) })
    })?;
    authors.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::permissions::Role;
    use crate::ops::testing::{fresh, give_role, member, TestCore};
    use crate::ops::tree::{create_page, NewEntry};

    /// A Page in an Area of its own, created by `writer`.
    fn page(core: &Core, writer: &Principal) -> String {
        let area = create_area(core, writer, NewArea { name: "Engineering".into() }).unwrap().output;
        let input = NewEntry { area_id: area.id, parent_id: None, title: "Handbook".into() };
        create_page(core, writer, input.into()).unwrap().output.id
    }

    fn id_of(principal: &Principal) -> &str {
        let Principal::Member { id, .. } = principal else { panic!("not a Member") };
        id
    }

    /// Has `author` change the Page to say the given Markdown, as an editor would.
    fn edit(core: &TestCore, page_id: &str, author: &Principal, markdown: &str) {
        let page = core.live().open(page_id).unwrap();
        let author = crate::live::Author::member(id_of(author));
        core.live().write(&page, &crate::markdown::from_markdown(markdown), &author);
    }

    fn versions(core: &Core, reader: &Principal, page_id: &str) -> Vec<VersionSummary> {
        list_versions(core, reader, VersionsOfPage { page_id: page_id.into() }).unwrap().output.versions
    }

    fn names(version: &VersionSummary) -> Vec<&str> {
        version.authors.iter().map(|author| author.name.as_str()).collect()
    }

    #[test]
    fn a_version_is_recorded_when_the_last_connection_closes_after_a_change() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let page_id = page(&core, &grace);
        let open = core.live().open(&page_id).unwrap();
        core.live().connected(&open);
        core.live().connected(&open);

        edit(&core, &page_id, &grace, "First steps.");
        core.live().disconnected(&open).unwrap();
        assert_eq!(versions(&core, &grace, &page_id), []);
        core.live().disconnected(&open).unwrap();

        let recorded = versions(&core, &grace, &page_id);
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].number, 1);
        assert_eq!(recorded[0].recorded_at, 1_700_000_000);
        assert_eq!(names(&recorded[0]), ["Grace"]);
        assert_eq!(recorded[0].restored_from, None);
    }

    #[test]
    fn while_a_page_has_unrecorded_changes_a_version_is_recorded_every_two_minutes() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let page_id = page(&core, &grace);
        let open = core.live().open(&page_id).unwrap();
        core.live().connected(&open);

        edit(&core, &page_id, &grace, "One.");
        core.advance(60);
        edit(&core, &page_id, &grace, "One. Two.");
        core.advance(59);
        core.live().record_due().unwrap();
        assert_eq!(versions(&core, &grace, &page_id), [], "119 seconds after the first change");

        core.advance(1);
        core.live().record_due().unwrap();
        let recorded = versions(&core, &grace, &page_id);
        assert_eq!(recorded.len(), 1, "two minutes after the first change");
        assert_eq!(recorded[0].recorded_at, 1_700_000_120);

        // The next two minutes start with the next change, not with the Version.
        core.advance(300);
        edit(&core, &page_id, &grace, "One. Two. Three.");
        core.advance(119);
        core.live().record_due().unwrap();
        assert_eq!(versions(&core, &grace, &page_id).len(), 1);
        core.advance(1);
        core.live().record_due().unwrap();
        let recorded = versions(&core, &grace, &page_id);
        assert_eq!(recorded.iter().map(|v| (v.number, v.recorded_at)).collect::<Vec<_>>(), [
            (2, 1_700_000_540),
            (1, 1_700_000_120)
        ]);
    }

    #[test]
    fn no_version_is_recorded_when_nothing_changed_since_the_last_one() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let page_id = page(&core, &grace);
        let open = core.live().open(&page_id).unwrap();

        // Opened and closed without a change.
        core.live().connected(&open);
        core.live().disconnected(&open).unwrap();
        assert_eq!(versions(&core, &grace, &page_id), []);

        core.live().connected(&open);
        edit(&core, &page_id, &grace, "Kept.");
        core.live().disconnected(&open).unwrap();
        assert_eq!(versions(&core, &grace, &page_id).len(), 1);

        // Changed, then changed back.
        core.live().connected(&open);
        edit(&core, &page_id, &grace, "Kept. For a moment.");
        edit(&core, &page_id, &grace, "Kept.");
        core.advance(120);
        core.live().record_due().unwrap();
        core.live().disconnected(&open).unwrap();
        assert_eq!(versions(&core, &grace, &page_id).len(), 1);
    }

    #[test]
    fn a_version_names_the_people_whose_changes_arrived_since_the_previous_one() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ada = member(&core, "Ada", false);
        let linus = member(&core, "Linus", false);
        let page_id = page(&core, &grace);
        let open = core.live().open(&page_id).unwrap();
        core.live().connected(&open);

        edit(&core, &page_id, &grace, "Grace was here.");
        edit(&core, &page_id, &linus, "Grace and Linus were here.");
        edit(&core, &page_id, &grace, "Grace and Linus were here!");
        core.advance(120);
        core.live().record_due().unwrap();
        edit(&core, &page_id, &ada, "Everyone was here.");
        core.live().disconnected(&open).unwrap();

        let recorded = versions(&core, &grace, &page_id);
        assert_eq!(names(&recorded[0]), ["Ada"]);
        assert_eq!(names(&recorded[1]), ["Grace", "Linus"]);
    }

    /// A Page by Grace with Versions 1 and 2, the first two lines of a list.
    fn page_with_two_versions(core: &TestCore, grace: &Principal) -> String {
        let page_id = page(core, grace);
        let open = core.live().open(&page_id).unwrap();
        core.live().connected(&open);
        edit(core, &page_id, grace, "# Packing\n\n- Laptop\n- Charger");
        core.live().disconnected(&open).unwrap();
        core.advance(60);
        core.live().connected(&open);
        edit(core, &page_id, grace, "# Packing\n\n- Laptop\n- Badge\n- Charger");
        core.live().disconnected(&open).unwrap();
        page_id
    }

    #[test]
    fn readers_list_and_view_versions_and_others_find_no_page() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let reader = member(&core, "Reader", false);
        let stranger = member(&core, "Stranger", false);
        let page_id = page_with_two_versions(&core, &grace);
        let area_id = crate::ops::tree::get_page(&core, &grace, PageId { id: page_id.clone() })
            .unwrap()
            .output
            .area
            .id;
        give_role(&core, &area_id, &reader, Role::Read);

        assert_eq!(versions(&core, &reader, &page_id).len(), 2);
        let first = get_version(&core, &reader, VersionOfPage { page_id: page_id.clone(), number: 1 })
            .unwrap()
            .output;
        assert_eq!(first.markdown, "# Packing\n\n- Laptop\n- Charger\n");
        assert_eq!(first.summary.number, 1);
        assert_eq!(first.summary.recorded_at, 1_700_000_000);

        let missing = get_version(&core, &reader, VersionOfPage { page_id: page_id.clone(), number: 3 });
        assert_eq!(missing.unwrap_err().message(), "There is no such Version.");

        let hidden = list_versions(&core, &stranger, VersionsOfPage { page_id: page_id.clone() });
        assert_eq!(hidden.unwrap_err().message(), "There is no such Page.");
        let hidden = get_version(&core, &stranger, VersionOfPage { page_id, number: 1 });
        assert_eq!(hidden.unwrap_err().message(), "There is no such Page.");
    }

    fn compared(core: &Core, reader: &Principal, page_id: &str, from: u32, to: u32) -> Vec<(Change, String)> {
        let input = TwoVersions { page_id: page_id.into(), from, to };
        let comparison = compare_versions(core, reader, input).unwrap().output;
        comparison.lines.into_iter().map(|line| (line.change, line.text)).collect()
    }

    #[test]
    fn comparing_two_versions_shows_the_lines_added_and_removed() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let page_id = page_with_two_versions(&core, &grace);
        let same = |text: &str| (Change::Same, text.to_string());
        let added = |text: &str| (Change::Added, text.to_string());
        let removed = |text: &str| (Change::Removed, text.to_string());

        assert_eq!(
            compared(&core, &grace, &page_id, 1, 2),
            [same("# Packing"), same(""), same("- Laptop"), added("- Badge"), same("- Charger")]
        );
        assert_eq!(
            compared(&core, &grace, &page_id, 2, 1),
            [same("# Packing"), same(""), same("- Laptop"), removed("- Badge"), same("- Charger")]
        );

        let missing = compare_versions(&core, &grace, TwoVersions { page_id, from: 1, to: 9 });
        assert_eq!(missing.unwrap_err().message(), "There is no such Version.");
    }

    #[test]
    fn a_changed_line_is_shown_as_removed_and_added() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let page_id = page(&core, &grace);
        let open = core.live().open(&page_id).unwrap();
        for markdown in ["Ship on Monday.\n\nTell support.", "Ship on Friday.\n\nTell support."] {
            core.live().connected(&open);
            edit(&core, &page_id, &grace, markdown);
            core.live().disconnected(&open).unwrap();
        }

        assert_eq!(
            compared(&core, &grace, &page_id, 1, 2),
            [
                (Change::Removed, "Ship on Monday.".to_string()),
                (Change::Added, "Ship on Friday.".to_string()),
                (Change::Same, "".to_string()),
                (Change::Same, "Tell support.".to_string()),
            ]
        );
    }

    fn restore(core: &Core, who: &Principal, page_id: &str, number: u32) -> Result<Restored, OpError> {
        restore_version(core, who, VersionOfPage { page_id: page_id.into(), number }).map(|reply| reply.output)
    }

    fn content(core: &TestCore, page_id: &str) -> String {
        let page = core.live().open(page_id).unwrap();
        crate::markdown::to_markdown(&crate::ydoc::read(&page.doc))
    }

    #[test]
    fn restoring_needs_write_and_a_page_the_caller_cannot_read_is_not_found() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let reader = member(&core, "Reader", false);
        let stranger = member(&core, "Stranger", false);
        let page_id = page_with_two_versions(&core, &grace);
        let area_id = crate::ops::tree::get_page(&core, &grace, PageId { id: page_id.clone() })
            .unwrap()
            .output
            .area
            .id;
        give_role(&core, &area_id, &reader, Role::Read);

        let refused = restore(&core, &reader, &page_id, 1).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
        assert_eq!(refused.message(), "Only someone with the Write Role can change this Page.");
        assert_eq!(restore(&core, &stranger, &page_id, 1).unwrap_err().message(), "There is no such Page.");
        assert_eq!(restore(&core, &grace, &page_id, 7).unwrap_err().message(), "There is no such Version.");
        assert_eq!(versions(&core, &grace, &page_id).len(), 2);
        assert_eq!(content(&core, &page_id), "# Packing\n\n- Laptop\n- Badge\n- Charger\n");
    }

    #[test]
    fn restoring_records_a_new_version_and_keeps_the_history() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ada = member(&core, "Ada", false);
        let page_id = page_with_two_versions(&core, &grace);
        let area_id = crate::ops::tree::get_page(&core, &grace, PageId { id: page_id.clone() })
            .unwrap()
            .output
            .area
            .id;
        give_role(&core, &area_id, &ada, Role::Write);
        // Someone is editing and has changes no Version holds yet.
        let open = core.live().open(&page_id).unwrap();
        core.live().connected(&open);
        edit(&core, &page_id, &grace, "# Packing\n\n- Laptop\n- Badge\n- Charger\n- Snacks");
        core.advance(30);

        let restored = restore(&core, &ada, &page_id, 1).unwrap();

        assert_eq!(content(&core, &page_id), "# Packing\n\n- Laptop\n- Charger\n");
        let recorded = versions(&core, &ada, &page_id);
        let listed: Vec<_> = recorded.iter().map(|v| (v.number, names(v), v.restored_from)).collect();
        assert_eq!(listed, [(4, vec!["Ada"], Some(1)), (3, vec!["Grace"], None), (2, vec!["Grace"], None), (1, vec!["Grace"], None)]);
        assert_eq!(restored.version, Some(recorded[0].clone()));
        assert_eq!(recorded[0].recorded_at, 1_700_000_090);
        let third = get_version(&core, &ada, VersionOfPage { page_id: page_id.clone(), number: 3 }).unwrap().output;
        assert_eq!(third.markdown, "# Packing\n\n- Laptop\n- Badge\n- Charger\n- Snacks\n");

        // The restore is not recorded again when the editor leaves.
        core.live().disconnected(&open).unwrap();
        assert_eq!(versions(&core, &ada, &page_id).len(), 4);
    }

    #[test]
    fn restoring_changes_only_what_differs_so_a_cursor_elsewhere_stays_put() {
        use yrs::{Assoc, IndexedSequence, Transact, XmlFragment, XmlOut};
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let page_id = page(&core, &grace);
        let open = core.live().open(&page_id).unwrap();
        for markdown in ["Introduction.\n\nOld ending.", "Introduction.\n\nNew ending."] {
            core.live().connected(&open);
            edit(&core, &page_id, &grace, markdown);
            core.live().disconnected(&open).unwrap();
        }
        let first_text = || {
            let fragment = open.doc.get_or_insert_xml_fragment("default");
            let txn = open.doc.transact();
            let Some(XmlOut::Element(paragraph)) = fragment.get(&txn, 0) else { panic!("no paragraph") };
            let Some(XmlOut::Text(text)) = paragraph.get(&txn, 0) else { panic!("no text") };
            text
        };
        // Someone's cursor sits after "Intro" in the paragraph both Versions share.
        let text = first_text();
        let cursor = text.sticky_index(&open.doc.transact_mut(), 5, Assoc::After).unwrap();

        restore(&core, &grace, &page_id, 1).unwrap();

        assert_eq!(content(&core, &page_id), "Introduction.\n\nOld ending.\n");
        assert_eq!(first_text(), text, "the shared paragraph was kept, not replaced");
        let offset = cursor.get_offset(&open.doc.transact()).unwrap();
        assert_eq!(offset.index, 5);
    }
}
