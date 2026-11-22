//! Tags: free-form labels on Pages, shared across the whole Workspace. A Tag
//! ignores upper and lower case and exists as long as one Page carries it.

use super::accounts::Nothing;
use super::areas;
use super::permissions;
use super::tree;
use super::{Catalogue, Core, OpError, OpResult, Principal};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "list_tags",
            "Lists the Tags in use on the Pages the caller can read, the most used first, to suggest when adding a Tag.",
            list_tags,
        )
        .changes = false;
    catalogue.add(
        "add_tag",
        "Adds a Tag to a Page and returns the Page's Tags. A Tag ignores upper and lower case: it is stored in lower case, without a leading #, with spaces turned into hyphens. Adding a Tag the Page already carries changes nothing. Needs the Write Role.",
        add_tag,
    );
    catalogue.add(
        "remove_tag",
        "Takes a Tag off a Page and returns the Page's Tags. Upper and lower case do not matter. Taking off a Tag the Page does not carry changes nothing. A Tag no Page carries any more no longer exists. Needs the Write Role.",
        remove_tag,
    );
}

/// The Tags in use.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TagsInUse {
    /// The one on the most Pages first; as many alphabetically.
    pub tags: Vec<String>,
}

/// A Tag on a Page.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PageTag {
    pub page_id: String,
    /// Upper and lower case do not matter.
    pub tag: String,
}

/// The Tags a Page carries.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PageTags {
    /// In alphabetical order.
    pub tags: Vec<String>,
}

pub fn list_tags(core: &Core, principal: &Principal, _input: Nothing) -> OpResult<TagsInUse> {
    areas::signed_in(principal)?;
    let db = core.db();
    // Hidden is not found: a Tag counts only on Pages the caller can read, so
    // one carried by no such Page is never named.
    let readable: HashSet<String> = permissions::readable_through(&db, principal)?.into_iter().collect();
    let mut statement = db.prepare("SELECT page_id, tag FROM page_tags")?;
    let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    let mut page_readable: HashMap<String, bool> = HashMap::new();
    let mut pages_per_tag: HashMap<String, usize> = HashMap::new();
    for row in rows {
        let (page_id, tag) = row?;
        let can_read = match page_readable.get(&page_id) {
            Some(known) => *known,
            None => {
                let governing = permissions::governing_node(&db, &page_id)?;
                let can_read = governing.is_some_and(|node| readable.contains(&node))
                    && !permissions::in_trash(&db, &page_id)?;
                page_readable.insert(page_id, can_read);
                can_read
            }
        };
        if can_read {
            *pages_per_tag.entry(tag).or_default() += 1;
        }
    }
    let mut counted: Vec<(String, usize)> = pages_per_tag.into_iter().collect();
    counted.sort_by(|(a, a_pages), (b, b_pages)| b_pages.cmp(a_pages).then_with(|| a.cmp(b)));
    let tags = counted.into_iter().map(|(tag, _)| tag).collect();
    Ok(TagsInUse { tags }.into())
}

pub fn add_tag(core: &Core, principal: &Principal, input: PageTag) -> OpResult<PageTags> {
    change_tags(core, principal, &input.page_id, |db| {
        let tag = valid_tag(&input.tag)?;
        db.execute(
            "INSERT OR IGNORE INTO page_tags (page_id, tag) VALUES (?1, ?2)",
            rusqlite::params![input.page_id, tag],
        )?;
        Ok(())
    })
}

pub fn remove_tag(core: &Core, principal: &Principal, input: PageTag) -> OpResult<PageTags> {
    change_tags(core, principal, &input.page_id, |db| {
        let tag = valid_tag(&input.tag)?;
        db.execute(
            "DELETE FROM page_tags WHERE page_id = ?1 AND tag = ?2",
            rusqlite::params![input.page_id, tag],
        )?;
        Ok(())
    })
}

/// Changes the Tags of a Page the principal may change, then returns them.
fn change_tags(
    core: &Core,
    principal: &Principal,
    page_id: &str,
    change: impl FnOnce(&Connection) -> Result<(), OpError>,
) -> OpResult<PageTags> {
    // Access is checked before the input, so an unreadable Page stays hidden
    // whatever Tag is sent.
    tree::page_to_change(core, principal, page_id)?;
    let db = core.db();
    change(&db)?;
    let tags = tags_of(&db, page_id)?;
    core.live().tags_changed(page_id, &tags);
    core.page_changed(page_id);
    Ok(PageTags { tags }.into())
}

/// The Tags of a Page, in alphabetical order. Does not check who may read them.
pub fn tags_of(db: &Connection, page_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut statement = db.prepare("SELECT tag FROM page_tags WHERE page_id = ?1 ORDER BY tag")?;
    let tags = statement.query_map([page_id], |row| row.get(0))?;
    tags.collect()
}

/// The Tag as it is stored: in lower case, without a leading #, with each run
/// of spaces turned into one hyphen, so "#Release Notes" is "release-notes".
/// Empty when nothing is left of it.
pub(super) fn stored_form(tag: &str) -> String {
    let words: Vec<String> = tag.trim().trim_start_matches('#').split_whitespace().map(str::to_lowercase).collect();
    words.join("-")
}

/// Long enough for any sensible Tag, short enough to sit beside a title.
const MAX_TAG_LENGTH: usize = 50;

/// The Tag as it is stored, refused when it is blank or too long.
fn valid_tag(tag: &str) -> Result<String, OpError> {
    let tag = stored_form(tag);
    if tag.is_empty() {
        return Err(OpError::InvalidInput("Enter a Tag.".into()));
    }
    if tag.chars().count() > MAX_TAG_LENGTH {
        return Err(OpError::InvalidInput(format!("Choose a Tag of at most {MAX_TAG_LENGTH} characters.")));
    }
    Ok(tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::permissions::Role;
    use crate::ops::testing::{fresh, give_role, member, personal_area};
    use crate::ops::TokenLimits;
    use crate::ops::tree::{create_page, get_page, NewEntry, PageId};
    use crate::ops::{Core, Principal};

    fn page(core: &Core, writer: &Principal, area_id: &str, title: &str) -> String {
        let entry = NewEntry { area_id: area_id.into(), parent_id: None, title: title.into() };
        create_page(core, writer, entry.into()).unwrap().output.id
    }

    fn area(core: &Core, admin: &Principal, name: &str) -> String {
        create_area(core, admin, NewArea { name: name.into() }).unwrap().output.id
    }

    fn tag(page_id: &str, tag: &str) -> PageTag {
        PageTag { page_id: page_id.into(), tag: tag.into() }
    }

    fn tags_seen_by(core: &Core, reader: &Principal, page_id: &str) -> Vec<String> {
        get_page(core, reader, PageId { id: page_id.into() }).unwrap().output.tags
    }

    #[test]
    fn a_tag_is_stored_in_lower_case_and_read_with_its_page() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let page_id = page(&core, &grace, &area_id, "Deploy");

        let added = add_tag(&core, &grace, tag(&page_id, "  Runbook ")).unwrap().output;
        add_tag(&core, &grace, tag(&page_id, "Production")).unwrap();

        assert_eq!(added.tags, ["runbook"]);
        assert_eq!(tags_seen_by(&core, &grace, &page_id), ["production", "runbook"]);
    }

    #[test]
    fn a_tag_is_removed_whatever_its_case_and_removing_one_the_page_lacks_changes_nothing() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let page_id = page(&core, &grace, &area_id, "Deploy");
        add_tag(&core, &grace, tag(&page_id, "runbook")).unwrap();
        add_tag(&core, &grace, tag(&page_id, "production")).unwrap();

        let removed = remove_tag(&core, &grace, tag(&page_id, "RunBook")).unwrap().output;
        let again = remove_tag(&core, &grace, tag(&page_id, "runbook")).unwrap().output;

        assert_eq!(removed.tags, ["production"]);
        assert_eq!(again.tags, ["production"]);
        assert_eq!(tags_seen_by(&core, &grace, &page_id), ["production"]);
    }

    fn in_use(core: &Core, principal: &Principal) -> Vec<String> {
        list_tags(core, principal, Nothing {}).unwrap().output.tags
    }

    #[test]
    fn the_tags_in_use_come_most_used_first_and_a_tag_ends_with_its_last_page() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let deploy = page(&core, &grace, &area_id, "Deploy");
        let rollback = page(&core, &grace, &area_id, "Rollback");
        add_tag(&core, &grace, tag(&deploy, "runbook")).unwrap();
        add_tag(&core, &grace, tag(&deploy, "production")).unwrap();
        add_tag(&core, &grace, tag(&rollback, "runbook")).unwrap();

        assert_eq!(in_use(&core, &grace), ["runbook", "production"]);
        remove_tag(&core, &grace, tag(&deploy, "production")).unwrap();
        assert_eq!(in_use(&core, &grace), ["runbook"]);
    }

    #[test]
    fn tags_of_pages_one_cannot_read_are_never_suggested() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let engineering = area(&core, &grace, "Engineering");
        let finance = area(&core, &grace, "Finance");
        give_role(&core, &engineering, &ken, Role::Read);
        add_tag(&core, &grace, tag(&page(&core, &grace, &engineering, "Deploy"), "runbook")).unwrap();
        add_tag(&core, &grace, tag(&page(&core, &grace, &finance, "Salaries"), "payroll")).unwrap();
        let own = personal_area(&core, &grace);
        add_tag(&core, &grace, tag(&page(&core, &grace, &own, "Diary"), "private")).unwrap();
        let admin = member(&core, "Ada", true);

        assert_eq!(in_use(&core, &ken), ["runbook"]);
        assert_eq!(in_use(&core, &admin), ["payroll", "runbook"], "not the Personal Area of another Member");
        assert_eq!(in_use(&core, &grace), ["payroll", "private", "runbook"]);
        let finance_page = page(&core, &grace, &finance, "Budget");
        let refused = add_tag(&core, &ken, tag(&finance_page, "")).unwrap_err();
        assert_eq!((refused.kind(), refused.message()), ("not_found", "There is no such Page."));
    }

    #[test]
    fn a_token_limited_to_chosen_areas_is_suggested_the_tags_of_those_alone() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let finance = area(&core, &grace, "Finance");
        add_tag(&core, &grace, tag(&page(&core, &grace, &engineering, "Deploy"), "runbook")).unwrap();
        add_tag(&core, &grace, tag(&page(&core, &grace, &finance, "Salaries"), "payroll")).unwrap();
        let Principal::Member { id, .. } = &grace else { unreachable!() };
        let through_token = Principal::Member {
            id: id.clone(),
            workspace_admin: false,
            session: None,
            token: Some(TokenLimits { id: "t1".into(), read_only: true, areas: Some(vec![engineering]) }),
        };

        assert_eq!(in_use(&core, &through_token), ["runbook"]);
    }

    #[test]
    fn a_tag_must_not_be_blank_or_too_long() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let page_id = page(&core, &grace, &area(&core, &grace, "Engineering"), "Deploy");

        let blank = add_tag(&core, &grace, tag(&page_id, " # ")).unwrap_err();
        let long = add_tag(&core, &grace, tag(&page_id, &"a".repeat(51))).unwrap_err();
        let spaced = add_tag(&core, &grace, tag(&page_id, "#Release   Notes")).unwrap().output;

        assert_eq!(blank.message(), "Enter a Tag.");
        assert_eq!(long.message(), "Choose a Tag of at most 50 characters.");
        assert_eq!(spaced.tags, ["release-notes"]);
    }

    #[test]
    fn people_with_the_page_open_receive_its_tags_whenever_they_change() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let page_id = page(&core, &grace, &area(&core, &grace, "Engineering"), "Deploy");
        let mut frames = core.live().open(&page_id).unwrap().frames.subscribe();

        add_tag(&core, &grace, tag(&page_id, "Runbook")).unwrap();
        add_tag(&core, &grace, tag(&page_id, "production")).unwrap();
        remove_tag(&core, &grace, tag(&page_id, "runbook")).unwrap();

        let mut sent = Vec::new();
        while let Ok((from, frame)) = frames.try_recv() {
            if frame[0] == crate::live::FRAME_TAGS {
                assert_eq!(from, crate::live::FROM_TURTLE);
                sent.push(serde_json::from_slice::<Vec<String>>(&frame[1..]).unwrap());
            }
        }
        assert_eq!(sent, [vec!["runbook"], vec!["production", "runbook"], vec!["production"]]);
    }
}
