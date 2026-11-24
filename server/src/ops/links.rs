//! Page links: links to another Page by its ID, shown with that Page's
//! current title to whoever may read it, and with nothing to anyone else.

use super::areas;
use crate::markdown::Linked;
use super::permissions;
use super::tree::Kind;
use super::{Catalogue, Core, OpResult, Principal};
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "get_page_titles",
            "Gets the current titles of the given Pages, as Page links show them. A Page the caller cannot read, or that is in the Trash, is left out, so its link shows no title.",
            get_page_titles,
        )
        .changes = false;
}

/// The most Pages one call asks about, a generous number for one Page's links.
const MAX_PAGES: usize = 500;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PagesToTitle {
    /// The IDs of the Pages, as Page links name them. Only the first 500 count.
    pub page_ids: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PageTitles {
    /// The Pages the caller can read, in the order they were asked for, each once.
    pub pages: Vec<PageTitle>,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PageTitle {
    pub id: String,
    pub title: String,
}

pub fn get_page_titles(core: &Core, principal: &Principal, input: PagesToTitle) -> OpResult<PageTitles> {
    areas::signed_in(principal)?;
    let db = core.db();
    let mut pages: Vec<PageTitle> = Vec::new();
    for id in input.page_ids.into_iter().take(MAX_PAGES) {
        if pages.iter().any(|page| page.id == id) {
            continue;
        }
        if let Some(title) = readable_title(&db, principal, &id)? {
            pages.push(PageTitle { id, title });
        }
    }
    Ok(PageTitles { pages }.into())
}

/// The title a Page link to the given Page shows the principal: the Page's
/// current title, or None when they cannot read it, it is in the Trash, or
/// there is no such Page.
pub(super) fn readable_title(db: &Connection, principal: &Principal, page_id: &str) -> rusqlite::Result<Option<String>> {
    let title: Option<String> = db
        .query_row(
            "SELECT title FROM nodes WHERE id = ?1 AND kind = ?2",
            rusqlite::params![page_id, Kind::Page.stored()],
            |row| row.get(0),
        )
        .optional()?;
    let Some(title) = title else { return Ok(None) };
    // role_on leaves out what is in the Trash.
    Ok(permissions::role_on(db, principal, page_id)?.map(|_| title))
}

/// The titles the principal may see of the Pages some content links to, by
/// ID; a Page they may not see is left out.
pub(super) fn linked_titles(
    core: &Core,
    principal: &Principal,
    blocks: &[crate::model::Block],
) -> rusqlite::Result<std::collections::HashMap<String, String>> {
    let db = core.db();
    let mut titles = std::collections::HashMap::new();
    for page_id in crate::model::linked_pages(blocks) {
        if let Some(title) = readable_title(&db, principal, &page_id)? {
            titles.insert(page_id, title);
        }
    }
    Ok(titles)
}

/// What the Page links and Mentions of some content show the principal.
pub(super) struct Shown {
    titles: std::collections::HashMap<String, String>,
    names: std::collections::HashMap<String, String>,
}

impl Shown {
    pub(super) fn of(core: &Core, principal: &Principal, blocks: &[crate::model::Block]) -> rusqlite::Result<Shown> {
        Ok(Shown { titles: linked_titles(core, principal, blocks)?, names: mentioned_names(core, blocks)? })
    }

    /// A Page link's title as `readable_title` gives it, or a Mentioned
    /// person's current name.
    pub(super) fn text(&self, linked: Linked) -> Option<String> {
        match linked {
            Linked::Page(page_id) => self.titles.get(page_id).cloned(),
            Linked::Person(person_id) => self.names.get(person_id).cloned(),
        }
    }
}

/// The current names of the people some content mentions, by ID. A Mention
/// shows the name to everyone who reads the content, whether or not they
/// may see the Profile; an ID that names no Member or Guest is left out.
pub(super) fn mentioned_names(
    core: &Core,
    blocks: &[crate::model::Block],
) -> rusqlite::Result<std::collections::HashMap<String, String>> {
    let db = core.db();
    let mut names = std::collections::HashMap::new();
    for person_id in crate::model::mentioned_people(blocks) {
        if let Some(name) = super::profiles::name_of(&db, &person_id)? {
            names.insert(person_id, name);
        }
    }
    Ok(names)
}

/// The Markdown of some content as the principal sees it, each Page link
/// and Mention with the text `Shown` gives it.
pub(super) fn markdown_for(core: &Core, principal: &Principal, blocks: &[crate::model::Block]) -> rusqlite::Result<String> {
    let shown = Shown::of(core, principal, blocks)?;
    Ok(crate::markdown::to_markdown_with_titles(blocks, &|linked| shown.text(linked)))
}

/// Stored Markdown, such as a Version's, as the principal sees it. Stored
/// Markdown holds no titles, so none is kept from the time it was written.
pub(super) fn stored_markdown_for(core: &Core, principal: &Principal, stored: &str) -> rusqlite::Result<String> {
    markdown_for(core, principal, &crate::markdown::from_markdown(stored))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::content::{get_page_markdown, replace_page, NewContent, OfPage};
    use crate::ops::permissions::Role;
    use crate::ops::testing::{fresh, give_role, member, personal_area};
    use crate::ops::trash::{move_to_trash, EntryId};
    use crate::ops::tree::{create_page, rename_page, NewEntry, RenameEntry};

    fn area(core: &Core, admin: &Principal, name: &str) -> String {
        create_area(core, admin, NewArea { name: name.into() }).unwrap().output.id
    }

    fn page(core: &Core, writer: &Principal, area_id: &str, title: &str) -> String {
        let input = NewEntry { area_id: area_id.into(), parent_id: None, title: title.into() };
        create_page(core, writer, input.into()).unwrap().output.id
    }

    fn markdown(core: &Core, reader: &Principal, page_id: &str) -> String {
        get_page_markdown(core, reader, OfPage { page_id: page_id.into() }).unwrap().output.markdown
    }

    fn write(core: &Core, writer: &Principal, page_id: &str, markdown: &str) {
        replace_page(core, writer, NewContent { page_id: page_id.into(), markdown: markdown.into() }).unwrap();
    }

    #[test]
    fn a_page_link_shows_the_current_title_and_follows_a_rename() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let handbook = page(&core, &grace, &engineering, "Handbook");
        let setup = page(&core, &grace, &engineering, "Setup");

        // Whatever the link said when written, it shows the Page's title.
        write(&core, &grace, &handbook, &format!("Read [the old name](turtle:{setup}) first."));
        assert_eq!(markdown(&core, &grace, &handbook), format!("Read [Setup](turtle:{setup}) first.\n"));

        rename_page(&core, &grace, RenameEntry { id: setup.clone(), title: "Getting started".into() }).unwrap();
        assert_eq!(markdown(&core, &grace, &handbook), format!("Read [Getting started](turtle:{setup}) first.\n"));
    }

    #[test]
    fn a_mention_is_written_with_the_current_name_whatever_it_said_and_whoever_reads_it() {
        use crate::ops::members::{deactivate_member, MemberId};
        use crate::ops::testing::guest;
        let core = fresh();
        let grace = member(&core, "Grace", true);
        member(&core, "Ken", false);
        let gus = guest(&core, "Gus");
        let engineering = area(&core, &grace, "Engineering");
        give_role(&core, &engineering, &gus, Role::Read);
        let handbook = page(&core, &grace, &engineering, "Handbook");

        write(&core, &grace, &handbook, "| Owner |\n| --- |\n| [@Kenny](turtle:member/member-ken) [@x](turtle:member/nobody) |");
        let expected = "| Owner |\n| --- |\n| [@Ken](turtle:member/member-ken) [@](turtle:member/nobody) |\n";
        assert_eq!(markdown(&core, &grace, &handbook), expected);
        // Ken shares no Area with Gus, so Gus may not see his Profile, but sees his name.
        assert_eq!(markdown(&core, &gus, &handbook), expected);

        deactivate_member(&core, &grace, MemberId { id: "member-ken".into() }).unwrap();
        assert_eq!(markdown(&core, &grace, &handbook), expected);
    }

    #[test]
    fn a_page_link_to_a_page_the_reader_cannot_read_or_in_the_trash_shows_no_title() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let engineering = area(&core, &grace, "Engineering");
        let secret_area = area(&core, &grace, "Board");
        give_role(&core, &engineering, &ken, Role::Read);
        let handbook = page(&core, &grace, &engineering, "Handbook");
        let secret = page(&core, &grace, &secret_area, "Layoffs");
        let old = page(&core, &grace, &engineering, "Old plan");
        write(&core, &grace, &handbook, &format!("[x](turtle:{secret}) [y](turtle:{old})"));

        assert_eq!(markdown(&core, &ken, &handbook), format!("[](turtle:{secret}) [Old plan](turtle:{old})\n"));
        move_to_trash(&core, &grace, EntryId { id: old.clone() }).unwrap();
        assert_eq!(markdown(&core, &ken, &handbook), format!("[](turtle:{secret}) [](turtle:{old})\n"));
        assert_eq!(markdown(&core, &grace, &handbook), format!("[Layoffs](turtle:{secret}) [](turtle:{old})\n"));
    }

    #[test]
    fn a_version_and_a_comparison_show_page_links_with_the_titles_the_reader_may_see() {
        use crate::ops::versions::{compare_versions, get_version, TwoVersions, VersionOfPage};
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let engineering = area(&core, &grace, "Engineering");
        let secret_area = area(&core, &grace, "Board");
        give_role(&core, &engineering, &ken, Role::Read);
        let handbook = page(&core, &grace, &engineering, "Handbook");
        let setup = page(&core, &grace, &engineering, "Setup");
        let secret = page(&core, &grace, &secret_area, "Layoffs");
        write(&core, &grace, &handbook, &format!("[a](turtle:{setup})"));
        write(&core, &grace, &handbook, &format!("[a](turtle:{setup}) [b](turtle:{secret})"));

        let first = VersionOfPage { page_id: handbook.clone(), number: 1 };
        assert_eq!(get_version(&core, &ken, first).unwrap().output.markdown, format!("[Setup](turtle:{setup})\n"));
        let both = TwoVersions { page_id: handbook, from: 1, to: 2 };
        let lines: Vec<String> = compare_versions(&core, &ken, both).unwrap().output.lines.into_iter().map(|line| line.text).collect();
        assert_eq!(lines, [format!("[Setup](turtle:{setup})"), format!("[Setup](turtle:{setup}) [](turtle:{secret})")]);
    }

    #[test]
    fn page_titles_are_given_only_for_pages_the_caller_can_read() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let engineering = area(&core, &grace, "Engineering");
        let secret_area = area(&core, &grace, "Board");
        give_role(&core, &engineering, &ken, Role::Read);
        let setup = page(&core, &grace, &engineering, "Setup");
        let secret = page(&core, &grace, &secret_area, "Layoffs");
        let old = page(&core, &grace, &engineering, "Old plan");
        move_to_trash(&core, &grace, EntryId { id: old.clone() }).unwrap();
        let private = page(&core, &grace, &personal_area(&core, &grace), "Diary");
        let ada = member(&core, "Ada", true);

        let asked = vec![secret, setup.clone(), old, private.clone(), "no-such-page".into(), setup.clone()];
        let titles = |who: &Principal| get_page_titles(&core, who, PagesToTitle { page_ids: asked.clone() }).unwrap().output;

        assert_eq!(titles(&ken), PageTitles { pages: vec![PageTitle { id: setup.clone(), title: "Setup".into() }] });
        // A Workspace Admin sees every Area but the Personal Areas of others.
        let for_ada: Vec<String> = titles(&ada).pages.into_iter().map(|page| page.title).collect();
        assert_eq!(for_ada, ["Layoffs", "Setup"]);
        assert_eq!(titles(&grace).pages.len(), 3, "her own Diary too");
    }
}
