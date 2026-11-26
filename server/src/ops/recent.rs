//! Recently edited Pages: where the caller, and the people they work with,
//! were writing last.

use super::areas::Area;
use super::permissions;
use super::tree::LastEdited;
use super::versions;
use super::{Catalogue, Core, OpError, OpResult, Principal};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "list_recent_pages",
            "Lists the Pages the caller can read whose newest Version is the most recent, newest first, whoever wrote it. A Page nobody has written in yet is left out, and so is what is in the Trash.",
            list_recent_pages,
        )
        .changes = false;
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecentPagesQuery {
    /// How many to list, from 1 to 50; 5 if absent.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RecentPageList {
    pub pages: Vec<RecentPage>,
}

/// A Page with when it was last edited and by whom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RecentPage {
    /// The ID of the Page, which opens with `get_page`.
    pub id: String,
    pub title: String,
    /// The Area the Page is in.
    pub area: Area,
    /// Its newest Version: when it was recorded and whose changes it holds.
    pub last_edited: LastEdited,
}

const DEFAULT_LIMIT: u32 = 5;
const MAX_LIMIT: u32 = 50;

pub fn list_recent_pages(core: &Core, principal: &Principal, input: RecentPagesQuery) -> OpResult<RecentPageList> {
    super::areas::signed_in(principal)?;
    let limit = input.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(OpError::InvalidInput(format!("Choose a limit from 1 to {MAX_LIMIT}.")));
    }
    let db = core.db();
    let mut statement = db.prepare(
        "SELECT nodes.id, nodes.title, nodes.area_id FROM nodes
         JOIN versions ON versions.page_id = nodes.id
         WHERE nodes.kind = 'page'
         GROUP BY nodes.id
         ORDER BY max(versions.recorded_at) DESC, max(versions.rowid) DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
    })?;
    let mut pages = Vec::new();
    for row in rows {
        let (id, title, area_id) = row?;
        // Hidden like any other Page: one the caller cannot read, reached
        // through a Token limited to other Areas, or in the Trash.
        if permissions::role_on(&db, principal, &id)?.is_none() {
            continue;
        }
        let area = match super::areas::readable_area(&db, principal, &area_id) {
            Ok(area) => area,
            Err(OpError::NotFound(_)) => continue,
            Err(error) => return Err(error),
        };
        let Some(newest) = versions::newest(&db, &id)? else { continue };
        pages.push(RecentPage {
            id,
            title,
            area,
            last_edited: LastEdited { at: newest.recorded_at, by: newest.authors },
        });
        if pages.len() == limit as usize {
            break;
        }
    }
    Ok(RecentPageList { pages }.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::content::{replace_page, NewContent};
    use crate::ops::permissions::Role;
    use crate::ops::testing::{fresh, give_role, member};
    use crate::ops::trash::{move_to_trash, EntryId};
    use crate::ops::tree::{create_page, NewEntry, NewPage};
    use crate::ops::versions::Author;

    fn titles(core: &Core, principal: &Principal, limit: Option<u32>) -> Vec<String> {
        list_recent_pages(core, principal, RecentPagesQuery { limit })
            .unwrap()
            .output
            .pages
            .into_iter()
            .map(|page| page.title)
            .collect()
    }

    fn page(core: &Core, author: &Principal, area_id: &str, title: &str, markdown: Option<&str>) -> String {
        let entry = NewEntry { area_id: area_id.into(), parent_id: None, title: title.into() };
        create_page(core, author, NewPage { entry, markdown: markdown.map(Into::into) }).unwrap().output.id
    }

    #[test]
    fn pages_come_newest_edit_first_whoever_wrote_them() {
        let core = fresh();
        let grace = member(&core, "Grace", true);
        let dennis = member(&core, "Dennis", false);
        let area = create_area(&core, &grace, NewArea { name: "Engineering".into() }).unwrap().output;
        give_role(&core, &area.id, &dennis, Role::Write);
        let runbook = page(&core, &grace, &area.id, "Runbook", Some("Restart it."));
        core.advance(60);
        page(&core, &grace, &area.id, "Roadmap", Some("Ship it."));
        core.advance(60);
        page(&core, &grace, &area.id, "Empty", None);
        core.advance(60);
        replace_page(&core, &dennis, NewContent { page_id: runbook.clone(), markdown: "Restart it twice.".into() })
            .unwrap();

        assert_eq!(titles(&core, &grace, None), ["Runbook", "Roadmap"], "a Page nobody wrote in is left out");
        let listed = list_recent_pages(&core, &grace, RecentPagesQuery { limit: Some(1) }).unwrap().output.pages;
        assert_eq!(listed[0].area.name, "Engineering");
        assert_eq!(
            listed[0].last_edited,
            LastEdited {
                at: 1_700_000_180,
                by: vec![Author::Member { id: "member-dennis".into(), name: "Dennis".into() }],
            }
        );
    }

    #[test]
    fn what_the_caller_cannot_read_or_is_in_the_trash_is_left_out() {
        let core = fresh();
        let grace = member(&core, "Grace", true);
        let ken = member(&core, "Ken", false);
        let shared = create_area(&core, &grace, NewArea { name: "Shared".into() }).unwrap().output;
        let closed = create_area(&core, &grace, NewArea { name: "Closed".into() }).unwrap().output;
        give_role(&core, &shared.id, &ken, Role::Read);
        page(&core, &grace, &shared.id, "Handbook", Some("Read me."));
        core.advance(60);
        page(&core, &grace, &closed.id, "Salaries", Some("Secret."));
        core.advance(60);
        let old = page(&core, &grace, &shared.id, "Old plan", Some("Outdated."));
        move_to_trash(&core, &grace, EntryId { id: old }).unwrap();

        assert_eq!(titles(&core, &ken, None), ["Handbook"]);
        assert_eq!(titles(&core, &grace, None), ["Salaries", "Handbook"]);
    }

    #[test]
    fn the_limit_is_kept_and_checked() {
        let core = fresh();
        let grace = member(&core, "Grace", true);
        let area = create_area(&core, &grace, NewArea { name: "Notes".into() }).unwrap().output;
        for title in ["One", "Two", "Three"] {
            page(&core, &grace, &area.id, title, Some("Text."));
            core.advance(1);
        }

        assert_eq!(titles(&core, &grace, Some(2)), ["Three", "Two"]);
        for limit in [0, 51] {
            assert!(matches!(
                list_recent_pages(&core, &grace, RecentPagesQuery { limit: Some(limit) }).map(|_| ()),
                Err(OpError::InvalidInput(_))
            ));
        }
    }
}
