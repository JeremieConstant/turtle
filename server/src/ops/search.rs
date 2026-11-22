//! Searching the Pages a principal can read, and keeping the search index
//! fed with what it needs to know about each Page.

use super::areas;
use super::permissions;
use super::{Catalogue, Core, OpResult, Principal};
use crate::search::{Changes, Fragment, PageDocument, Pending};
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue.add(
        "search",
        "Searches the titles and content of the Pages the caller can read, in one Area or everywhere. Tolerates typos and English and German word forms, and ranks matches in a title first. Pages the caller cannot read never appear.",
        search,
    );
}

/// The most results one search returns.
const MAX_RESULTS: usize = 20;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchQuery {
    /// What to look for, as typed. The last word also matches words it is
    /// the beginning of, so results can be shown while typing.
    pub query: String,
    /// The Area to search in; everything the caller can read if absent.
    #[serde(default)]
    pub area_id: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SearchResults {
    /// The best match first, at most 20.
    pub results: Vec<SearchResult>,
}

/// A Page that matched.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SearchResult {
    pub page_id: String,
    pub title: String,
    pub area_id: String,
    pub area_name: String,
    /// The titles of the Pages and Folders above the Page, from the top of
    /// its Area down.
    pub path: Vec<String>,
    /// A passage of the content around the first match, in pieces; the
    /// highlighted pieces are the words that matched. The beginning of the
    /// content when only the title matched, and empty for a Page without
    /// content. A passage cut from a longer text begins or ends with "…".
    pub snippet: Vec<Fragment>,
}

pub fn search(core: &Core, principal: &Principal, input: SearchQuery) -> OpResult<SearchResults> {
    areas::signed_in(principal)?;
    let readable = {
        let db = core.db();
        if let Some(area_id) = &input.area_id {
            areas::readable_area(&db, principal, area_id)?;
        }
        permissions::readable_through(&db, principal)?
    };
    // The index applies the access itself, so what the caller cannot read is
    // never among the hits, not even to be dropped later.
    let hits = core.search.search(&crate::search::Search {
        text: &input.query,
        area_id: input.area_id.as_deref(),
        readable: Some(&readable),
        limit: MAX_RESULTS,
    })?;
    let db = core.db();
    let mut results = Vec::new();
    for hit in hits {
        // A Page removed since it was indexed is skipped.
        let Some((title, area_id, area_name)) = db
            .query_row(
                "SELECT nodes.title, areas.id, areas.name
                 FROM nodes JOIN areas ON areas.id = nodes.area_id
                 WHERE nodes.id = ?1",
                [&hit.page_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
        else {
            continue;
        };
        let path = path_to(&db, &hit.page_id)?;
        results.push(SearchResult { page_id: hit.page_id, title, area_id, area_name, path, snippet: hit.snippet });
    }
    Ok(SearchResults { results }.into())
}

/// The titles above a node, from the top of its Area down.
fn path_to(db: &Connection, node_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut statement = db.prepare(
        "WITH RECURSIVE upward (id, parent_id, title, depth) AS (
             SELECT id, parent_id, title, 0 FROM nodes WHERE id = ?1
             UNION ALL
             SELECT nodes.id, nodes.parent_id, nodes.title, upward.depth + 1
             FROM nodes JOIN upward ON nodes.id = upward.parent_id
         )
         SELECT title FROM upward
         WHERE depth > 0 AND parent_id IS NOT NULL
         ORDER BY depth DESC",
    )?;
    let titles = statement.query_map([node_id], |row| row.get(0))?;
    titles.collect()
}

/// The documents for what is pending, read from the database.
pub(super) fn changes(db: &Connection, pending: &Pending) -> anyhow::Result<Changes> {
    let everything = if pending.everything {
        let mut statement = db.prepare("SELECT id FROM nodes WHERE kind = 'page' ORDER BY id")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut documents = Vec::new();
        for id in ids {
            documents.extend(document(db, &id)?);
        }
        Some(documents)
    } else {
        None
    };
    let mut pages = Vec::new();
    for id in &pending.pages {
        pages.push((id.clone(), document(db, id)?));
    }
    Ok(Changes { everything, pages })
}

/// A Page as the index needs it, or none when there is no such Page.
fn document(db: &Connection, page_id: &str) -> anyhow::Result<Option<PageDocument>> {
    let found = db
        .query_row(
            "SELECT nodes.area_id, nodes.title, pages.state
             FROM nodes LEFT JOIN pages ON pages.id = nodes.id
             WHERE nodes.id = ?1 AND nodes.kind = 'page'",
            [page_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<Vec<u8>>>(2)?)),
        )
        .optional()?;
    let Some((area_id, title, state)) = found else { return Ok(None) };
    let access = permissions::governing_node(db, page_id)?
        .ok_or_else(|| anyhow::anyhow!("Page {page_id} is in no Area"))?;
    let text = match state {
        Some(state) => crate::model::plain_text(&crate::ydoc::read_state(&state)?),
        None => String::new(),
    };
    Ok(Some(PageDocument { page_id: page_id.to_string(), area_id, access, title, text }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::OpError;
    use crate::ops::areas::{create_area, Area, NewArea};
    use crate::ops::permissions::{Holder, Role};
    use crate::ops::testing::{fresh, give_role, member};
    use crate::ops::tree::{create_folder, create_page, rename_page, NewEntry, RenameEntry};
    use crate::search::Fragment;

    fn area(core: &Core, admin: &Principal, name: &str) -> Area {
        create_area(core, admin, NewArea { name: name.into() }).unwrap().output
    }

    fn page(core: &Core, writer: &Principal, area: &Area, title: &str) -> String {
        let input = NewEntry { area_id: area.id.clone(), parent_id: None, title: title.into() };
        create_page(core, writer, input.into()).unwrap().output.id
    }

    /// Gives a Page content, as the editing server does when it saves one.
    fn write(core: &Core, page_id: &str, markdown: &str) {
        use yrs::{ReadTxn, StateVector, Transact};
        let doc = yrs::Doc::new();
        crate::ydoc::write(&doc, &crate::markdown::from_markdown(markdown));
        let state = doc.transact().encode_state_as_update_v1(&StateVector::default());
        core.db()
            .execute(
                "INSERT INTO pages (id, state) VALUES (?1, ?2)
                 ON CONFLICT (id) DO UPDATE SET state = excluded.state",
                rusqlite::params![page_id, state],
            )
            .unwrap();
        core.page_changed(page_id);
    }

    /// The titles found, in order, once the index has caught up.
    fn found(core: &Core, principal: &Principal, query: &str) -> Vec<String> {
        core.catch_up_search().unwrap();
        let input = SearchQuery { query: query.into(), area_id: None };
        let results = search(core, principal, input).unwrap().output.results;
        results.into_iter().map(|result| result.title).collect()
    }

    #[test]
    fn a_page_is_found_by_a_word_of_its_title() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        page(&core, &grace, &engineering, "Onboarding checklist");
        page(&core, &grace, &engineering, "Holidays");

        assert_eq!(found(&core, &grace, "onboarding"), ["Onboarding checklist"]);
    }

    #[test]
    fn a_page_is_found_by_a_word_of_its_content() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let setup = page(&core, &grace, &engineering, "Setup");
        page(&core, &grace, &engineering, "Holidays");

        write(&core, &setup, "# Laptop\n\nInstall **Docker** and the VPN client.\n");

        assert_eq!(found(&core, &grace, "docker"), ["Setup"]);
        assert_eq!(found(&core, &grace, "VPN laptop"), ["Setup"]);
        assert!(found(&core, &grace, "kubernetes").is_empty());
    }

    #[test]
    fn a_query_with_a_typo_still_finds_the_page() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let setup = page(&core, &grace, &engineering, "Onboarding checklist");
        write(&core, &setup, "Ask for a hardware token on the first day.\n");

        for query in ["onbaording", "onboardnig checklist", "hardwrae", "chekclist"] {
            assert_eq!(found(&core, &grace, query), ["Onboarding checklist"], "{query}");
        }
    }

    #[test]
    fn word_forms_match_in_english_and_german() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let release = page(&core, &grace, &engineering, "Release");
        let library = page(&core, &grace, &engineering, "Bibliothek");
        write(&core, &release, "We deployed the services on Friday.\n");
        write(&core, &library, "Jedes Buch steht im Regal.\n");

        // Too far apart to be typos, and neither begins the other.
        assert_eq!(found(&core, &grace, "deployment"), ["Release"]);
        assert_eq!(found(&core, &grace, "Bücher"), ["Bibliothek"]);
    }

    #[test]
    fn a_word_still_being_typed_finds_the_pages_it_begins() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let setup = page(&core, &grace, &engineering, "Onboarding checklist");
        write(&core, &setup, "Ask for a hardware token on the first day.\n");
        page(&core, &grace, &engineering, "Holidays");

        for query in ["o", "onb", "checklist onbo", "hardw", "first da"] {
            assert_eq!(found(&core, &grace, query), ["Onboarding checklist"], "{query}");
        }
        // Only the last word is still being typed.
        assert!(found(&core, &grace, "onb checklist").is_empty());
    }

    #[test]
    fn a_match_in_a_title_ranks_above_a_match_in_the_text() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let setup = page(&core, &grace, &engineering, "Setup");
        write(&core, &setup, "Docker.\n\nDocker, docker and docker again: install Docker.\n");
        let notes = page(&core, &grace, &engineering, "Docker notes");
        write(&core, &notes, "A long page about something else entirely, mentioning containers once.\n");

        for query in ["docker", "dokcer", "dock"] {
            assert_eq!(found(&core, &grace, query), ["Docker notes", "Setup"], "{query}");
        }
    }

    #[test]
    fn pages_the_searcher_cannot_read_never_appear() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let secret = area(&core, &grace, "Secret project");
        let open = area(&core, &grace, "Handbook");
        let everyone = area(&core, &grace, "Announcements");
        let hidden = page(&core, &grace, &secret, "Roadmap 2027");
        write(&core, &hidden, "The roadmap nobody else may see.\n");
        page(&core, &grace, &open, "Roadmap overview");
        page(&core, &grace, &everyone, "Roadmap news");
        let linus = member(&core, "Linus", false);
        let ada = member(&core, "Ada", true);
        give_role(&core, &open.id, &linus, Role::Read);
        let root = crate::ops::tree::root_of(&core.db(), &everyone.id).unwrap();
        permissions::grant(&core.db(), &root, &Holder::AllMembers, Role::Read).unwrap();

        let seen = |principal| {
            let mut titles = found(&core, principal, "roadmap");
            titles.sort();
            titles
        };
        assert_eq!(seen(&grace), ["Roadmap 2027", "Roadmap news", "Roadmap overview"]);
        assert_eq!(seen(&ada), ["Roadmap 2027", "Roadmap news", "Roadmap overview"]);
        assert_eq!(seen(&linus), ["Roadmap news", "Roadmap overview"]);
        let outsider = member(&core, "Ken", false);
        assert_eq!(found(&core, &outsider, "nobody"), Vec::<String>::new());
        assert_eq!(seen(&outsider), ["Roadmap news"]);
    }

    #[test]
    fn pages_in_a_personal_area_are_found_by_its_member_alone() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let linus = member(&core, "Linus", false);
        let ada = member(&core, "Ada", true);
        let personal = crate::ops::testing::personal_area(&core, &grace);
        let personal = crate::ops::areas::get_area(&core, &grace, crate::ops::areas::AreaId { id: personal })
            .unwrap()
            .output;
        let note = page(&core, &grace, &personal, "Salary thoughts");
        write(&core, &note, "Ask for a raise in March.\n");
        let handbook = area(&core, &ada, "Handbook");
        page(&core, &ada, &handbook, "Salary bands");
        give_role(&core, &handbook.id, &grace, Role::Read);
        give_role(&core, &handbook.id, &linus, Role::Read);

        let seen = |principal| {
            let mut titles = found(&core, principal, "salary");
            titles.sort();
            titles
        };
        assert_eq!(seen(&grace), ["Salary bands", "Salary thoughts"]);
        assert_eq!(seen(&linus), ["Salary bands"]);
        assert_eq!(seen(&ada), ["Salary bands"]);
        assert!(found(&core, &ada, "raise").is_empty());

        // A Token limited to chosen Areas searches only those.
        let limited = crate::ops::tokens::NewToken {
            name: "Handbook only".into(),
            read_only: false,
            area_ids: Some(vec![handbook.id.clone()]),
            expires_at: None,
            agent_id: None,
        };
        let created = crate::ops::tokens::create_token(&core, &grace, limited).unwrap().output;
        let acting = crate::ops::tokens::principal_for_token(&core, &created.value).unwrap().unwrap();
        assert_eq!(seen(&acting), ["Salary bands"]);
    }

    #[test]
    fn nobody_signed_in_cannot_search() {
        let core = fresh();
        let input = SearchQuery { query: "roadmap".into(), area_id: None };
        let refused = search(&core, &Principal::Anonymous, input).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
    }

    fn found_in(core: &Core, principal: &Principal, area: &str, query: &str) -> Result<Vec<String>, OpError> {
        core.catch_up_search().unwrap();
        let input = SearchQuery { query: query.into(), area_id: Some(area.into()) };
        let results = search(core, principal, input)?.output.results;
        Ok(results.into_iter().map(|result| result.title).collect())
    }

    #[test]
    fn a_search_in_an_area_finds_only_its_pages() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let sales = area(&core, &grace, "Sales");
        page(&core, &grace, &engineering, "Pricing engine");
        page(&core, &grace, &sales, "Pricing");

        assert_eq!(found_in(&core, &grace, &engineering.id, "pricing").unwrap(), ["Pricing engine"]);
        assert_eq!(found_in(&core, &grace, &sales.id, "pricing").unwrap(), ["Pricing"]);
        assert_eq!(found(&core, &grace, "pricing").len(), 2);
    }

    #[test]
    fn a_search_in_an_area_one_cannot_read_answers_like_one_that_does_not_exist() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let secret = area(&core, &grace, "Secret project");
        page(&core, &grace, &secret, "Roadmap");
        let linus = member(&core, "Linus", false);

        let hidden = found_in(&core, &linus, &secret.id, "roadmap").unwrap_err();
        let missing = found_in(&core, &linus, "no-such-area", "roadmap").unwrap_err();

        assert_eq!(hidden.kind(), "not_found");
        assert_eq!(hidden.message(), missing.message());
    }

    fn results(core: &Core, principal: &Principal, query: &str) -> Vec<SearchResult> {
        core.catch_up_search().unwrap();
        let input = SearchQuery { query: query.into(), area_id: None };
        search(core, principal, input).unwrap().output.results
    }

    fn piece(text: &str, highlighted: bool) -> Fragment {
        Fragment { text: text.into(), highlighted }
    }

    fn highlighted(result: &SearchResult) -> Vec<&str> {
        result.snippet.iter().filter(|piece| piece.highlighted).map(|piece| piece.text.as_str()).collect()
    }

    #[test]
    fn a_result_shows_its_area_its_place_in_the_tree_and_the_match_in_context() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let guides = create_folder(
            &core,
            &grace,
            NewEntry { area_id: engineering.id.clone(), parent_id: None, title: "Guides".into() },
        )
        .unwrap()
        .output;
        let setup = create_page(
            &core,
            &grace,
            NewEntry { area_id: engineering.id.clone(), parent_id: Some(guides.id), title: "Setup".into() }.into(),
        )
        .unwrap()
        .output;
        let laptop = create_page(
            &core,
            &grace,
            NewEntry { area_id: engineering.id.clone(), parent_id: Some(setup.id), title: "Laptop".into() }.into(),
        )
        .unwrap()
        .output;
        write(&core, &laptop.id, "Every new colleague gets a laptop.\n\nInstall *Docker* before the first standup.\n");

        let found = results(&core, &grace, "docker");

        assert_eq!(found.len(), 1);
        let result = &found[0];
        assert_eq!(result.page_id, laptop.id);
        assert_eq!(result.title, "Laptop");
        assert_eq!(result.area_id, engineering.id);
        assert_eq!(result.area_name, "Engineering");
        assert_eq!(result.path, ["Guides", "Setup"]);
        assert_eq!(
            result.snippet,
            [
                piece("Every new colleague gets a laptop. Install ", false),
                piece("Docker", true),
                piece(" before the first standup.", false),
            ]
        );
    }

    #[test]
    fn a_snippet_highlights_typos_word_forms_and_words_being_typed_in_a_window_of_a_long_text() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let release = page(&core, &grace, &engineering, "Release");
        let filler = "lorem ipsum dolor sit amet ".repeat(20);
        write(&core, &release, &format!("{filler}We deployed Docker on Friday. {filler}\n"));

        for (query, expected) in [("dokcer", ["Docker"]), ("deployment", ["deployed"]), ("frid", ["Friday"])] {
            let found = results(&core, &grace, query);
            let snippet = &found[0].snippet;
            assert_eq!(highlighted(&found[0]), expected, "{query}");
            let text: String = snippet.iter().map(|piece| piece.text.as_str()).collect();
            assert!(text.starts_with('…') && text.ends_with('…'), "{query}: {text}");
            assert!(text.chars().count() < 300, "{query}: {text}");
        }
    }

    #[test]
    fn a_match_only_in_the_title_shows_the_beginning_of_the_text() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let setup = page(&core, &grace, &engineering, "Docker");
        write(&core, &setup, "Install it first.\n");
        page(&core, &grace, &engineering, "Docker notes");

        let found = results(&core, &grace, "docker");

        let snippet = |title: &str| &found.iter().find(|result| result.title == title).unwrap().snippet;
        assert_eq!(snippet("Docker"), &[piece("Install it first.", false)]);
        assert!(snippet("Docker notes").is_empty());
    }

    #[test]
    fn a_renamed_or_rewritten_page_is_found_as_it_is_now() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let notes = page(&core, &grace, &engineering, "Notes");
        write(&core, &notes, "Discussed the budget.\n");
        assert_eq!(found(&core, &grace, "budget"), ["Notes"]);

        rename_page(&core, &grace, RenameEntry { id: notes.clone(), title: "Minutes".into() }).unwrap();
        write(&core, &notes, "Discussed the hiring plan.\n");

        assert_eq!(found(&core, &grace, "minutes"), ["Minutes"]);
        assert_eq!(found(&core, &grace, "hiring"), ["Minutes"]);
        assert!(found(&core, &grace, "notes").is_empty());
        assert!(found(&core, &grace, "budget").is_empty());
    }

    #[test]
    fn a_missing_index_is_rebuilt_from_the_database_on_start() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("turtle.db");
        let grace;
        {
            let core = Core::open(&db_path).unwrap();
            grace = member(&core, "Grace", false);
            let engineering = area(&core, &grace, "Engineering");
            let setup = page(&core, &grace, &engineering, "Setup");
            write(&core, &setup, "Install Docker.\n");
            core.catch_up_search().unwrap();
        }
        std::fs::remove_dir_all(dir.path().join("search")).unwrap();

        let core = Core::open(&db_path).unwrap();

        assert_eq!(found(&core, &grace, "docker"), ["Setup"]);
    }

    #[test]
    fn changes_not_yet_indexed_when_turtle_stopped_are_found_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("turtle.db");
        let grace;
        {
            let core = Core::open(&db_path).unwrap();
            grace = member(&core, "Grace", false);
            let engineering = area(&core, &grace, "Engineering");
            core.catch_up_search().unwrap();
            page(&core, &grace, &engineering, "Budget");
            core.close_search().unwrap();
        }
        {
            let core = Core::open(&db_path).unwrap();
            assert_eq!(found(&core, &grace, "budget"), ["Budget"]);
            let engineering = area(&core, &grace, "Sales");
            page(&core, &grace, &engineering, "Pricing");
            // Dropped without closing, as when the process is killed.
        }

        let core = Core::open(&db_path).unwrap();

        assert_eq!(found(&core, &grace, "budget"), ["Budget"]);
        assert_eq!(found(&core, &grace, "pricing"), ["Pricing"]);
    }
}
