//! Quick Notes: Pages a Member writes without choosing a place for them.

use super::areas::Area;
use super::permissions;
use super::tree;
use super::{Catalogue, Core, OpError, OpResult, Principal};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue.add(
        "create_quick_note",
        "Writes a Quick Note: a Page at the top of an Area, the caller's Personal Area unless another is chosen. Only a Member can, also through a Token that reaches that Area.",
        create_quick_note,
    );
    catalogue
        .add(
            "list_quick_notes",
            "Lists the caller's most recent Quick Notes, newest first, each with the Area it sits in now, also after it was moved. Those the caller can no longer read are left out. Empty for anyone without a Personal Area.",
            list_quick_notes,
        )
        .changes = false;
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct NewQuickNote {
    pub title: String,
    /// What the Quick Note says, in Markdown; empty if absent.
    #[serde(default)]
    pub markdown: Option<String>,
    /// What the Quick Note says as the web app's editor holds it: a Yjs
    /// document update, in base64. The web app sends it in place of
    /// `markdown`; everyone else writes `markdown`.
    #[serde(default)]
    pub editor_state: Option<String>,
    /// The Area it lands at the top of, which takes the Write Role; the
    /// caller's Personal Area if absent.
    #[serde(default)]
    pub area_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecentQuickNotes {
    /// How many to list, from 1 to 50; 5 if absent.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct QuickNoteList {
    pub quick_notes: Vec<QuickNote>,
}

/// A Quick Note, wherever it sits now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct QuickNote {
    /// The ID of its Page, which opens with `get_page`.
    pub id: String,
    pub title: String,
    /// When it was written, in seconds since the Unix epoch.
    pub created_at: i64,
    /// The Area it sits in now.
    pub area: Area,
}

pub fn create_quick_note(core: &Core, principal: &Principal, input: NewQuickNote) -> OpResult<QuickNote> {
    let member_id = super::areas::signed_in_member(
        principal,
        "A standalone Agent has no Personal Area, so it cannot write Quick Notes.",
    )?;
    let area = match &input.area_id {
        Some(area_id) => super::areas::readable_area(&core.db(), principal, area_id)?,
        None => super::areas::personal_area_of(&core.db(), principal)?.ok_or_else(|| {
            OpError::NotPermitted(if principal.token().is_some() {
                "This Token does not reach your Personal Area, where Quick Notes land.".into()
            } else {
                "You have no Personal Area, where Quick Notes land.".into()
            })
        })?,
    };
    let content = match (input.markdown, input.editor_state) {
        (Some(_), Some(_)) => {
            return Err(OpError::InvalidInput("Send the content either as Markdown or as editor state, not both.".into()))
        }
        (Some(markdown), None) => crate::markdown::from_markdown(&markdown),
        (None, Some(state)) => read_editor_state(&state)?,
        (None, None) => Vec::new(),
    };
    let created_at = core.now();
    let page = tree::create_quick_note_page(
        core,
        principal,
        tree::NewEntry { area_id: area.id.clone(), parent_id: None, title: input.title },
        content,
        member_id,
    )?
    .output;
    Ok(QuickNote { id: page.id, title: page.title, created_at, area }.into())
}

fn read_editor_state(state: &str) -> Result<Vec<crate::model::Block>, OpError> {
    use base64::Engine;
    let unreadable = || OpError::InvalidInput("The editor state could not be read. Reload the page and try again.".into());
    let bytes = base64::engine::general_purpose::STANDARD.decode(state).map_err(|_| unreadable())?;
    crate::ydoc::read_state(&bytes).map_err(|_| unreadable())
}

const DEFAULT_LIMIT: u32 = 5;
const MAX_LIMIT: u32 = 50;

pub fn list_quick_notes(core: &Core, principal: &Principal, input: RecentQuickNotes) -> OpResult<QuickNoteList> {
    super::areas::signed_in(principal)?;
    let limit = input.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(OpError::InvalidInput(format!("Choose a limit from 1 to {MAX_LIMIT}.")));
    }
    let Principal::Member { id: member_id, .. } = principal else {
        return Ok(QuickNoteList { quick_notes: Vec::new() }.into());
    };
    let db = core.db();
    let mut statement = db.prepare(
        "SELECT id, title, created_at, area_id FROM nodes
         WHERE quick_note_member_id = ?1 ORDER BY created_at DESC, rowid DESC",
    )?;
    let rows = statement.query_map([member_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, String>(3)?))
    })?;
    let mut quick_notes = Vec::new();
    for row in rows {
        let (id, title, created_at, area_id) = row?;
        // A Quick Note moved where its Member no longer reads, reached
        // through a Token limited to other Areas, or in the Trash, is hidden
        // like any other Page, and so is the name of an Area they cannot read.
        if permissions::role_on(&db, principal, &id)?.is_none() {
            continue;
        }
        let area = match super::areas::readable_area(&db, principal, &area_id) {
            Ok(area) => area,
            Err(OpError::NotFound(_)) => continue,
            Err(error) => return Err(error),
        };
        quick_notes.push(QuickNote { id, title, created_at, area });
        if quick_notes.len() == limit as usize {
            break;
        }
    }
    Ok(QuickNoteList { quick_notes }.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::content::{get_page_markdown, OfPage};
    use crate::ops::testing::{fresh, member, personal_area};
    use crate::ops::tree::{create_page, get_tree, NewEntry, TreeOfArea};

    fn quick_note(title: &str) -> NewQuickNote {
        NewQuickNote { title: title.into(), ..Default::default() }
    }

    #[test]
    fn a_quick_note_is_a_page_at_the_top_of_the_personal_area() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let personal = personal_area(&core, &grace);
        create_page(&core, &grace, NewEntry { area_id: personal.clone(), parent_id: None, title: "Older".into() }.into())
            .unwrap();

        let written = create_quick_note(
            &core,
            &grace,
            NewQuickNote {
                title: "  Call the printer ".into(),
                markdown: Some("Ask about the *invoice*.\n\nAnd the toner.".into()),
                ..Default::default()
            },
        )
        .unwrap()
        .output;

        assert_eq!(written.title, "Call the printer");
        assert_eq!(written.area.id, personal);
        assert!(written.area.personal);
        let tree = get_tree(&core, &grace, TreeOfArea { area_id: personal }).unwrap().output;
        let titles: Vec<_> = tree.entries.iter().map(|entry| entry.title.as_str()).collect();
        assert_eq!(titles, ["Call the printer", "Older"]);
        let page = get_page_markdown(&core, &grace, OfPage { page_id: written.id }).unwrap().output;
        assert_eq!(page.markdown.trim(), "Ask about the *invoice*.\n\nAnd the toner.");
    }

    fn listed(core: &Core, principal: &Principal) -> Vec<(String, i64)> {
        let list = list_quick_notes(core, principal, RecentQuickNotes { limit: None }).unwrap().output;
        list.quick_notes.into_iter().map(|quick_note| (quick_note.title, quick_note.created_at)).collect()
    }

    #[test]
    fn the_five_most_recent_quick_notes_are_listed_newest_first_and_other_pages_are_not() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let personal = personal_area(&core, &grace);
        for number in 1..=6 {
            create_quick_note(&core, &grace, quick_note(&format!("Note {number}"))).unwrap();
            core.advance(60);
        }
        create_page(&core, &grace, NewEntry { area_id: personal, parent_id: None, title: "Plain".into() }.into())
            .unwrap();

        assert_eq!(
            listed(&core, &grace),
            [
                ("Note 6".to_string(), 1_700_000_300),
                ("Note 5".to_string(), 1_700_000_240),
                ("Note 4".to_string(), 1_700_000_180),
                ("Note 3".to_string(), 1_700_000_120),
                ("Note 2".to_string(), 1_700_000_060),
            ]
        );
    }

    #[test]
    fn a_quick_note_needs_a_title_and_readable_content() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let both = NewQuickNote {
            title: "Idea".into(),
            markdown: Some("Text".into()),
            editor_state: Some(String::new()),
            ..Default::default()
        };
        let garbled = NewQuickNote { title: "Idea".into(), editor_state: Some("not base64!".into()), ..Default::default() };
        let table = [
            (quick_note(""), "Enter a title."),
            (quick_note("  "), "Enter a title."),
            (quick_note(&"x".repeat(201)), "Choose a title of at most 200 characters."),
            (both, "Send the content either as Markdown or as editor state, not both."),
            (garbled, "The editor state could not be read. Reload the page and try again."),
        ];
        for (input, message) in table {
            let refused = create_quick_note(&core, &grace, input).unwrap_err();
            assert_eq!((refused.kind(), refused.message()), ("invalid_input", message));
        }
        assert!(listed(&core, &grace).is_empty());
    }

    #[test]
    fn a_quick_note_holds_what_the_web_editor_wrote() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let doc = yrs::Doc::new();
        let update = crate::ydoc::write(&doc, &crate::markdown::from_markdown("- Ask about the **invoice**\n- And the toner")).unwrap();
        let editor_state = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, update);

        let written = create_quick_note(
            &core,
            &grace,
            NewQuickNote { title: "Call the printer".into(), editor_state: Some(editor_state), ..Default::default() },
        )
        .unwrap()
        .output;

        let page = get_page_markdown(&core, &grace, OfPage { page_id: written.id }).unwrap().output;
        assert_eq!(page.markdown.trim(), "- Ask about the **invoice**\n- And the toner");
    }

    #[test]
    fn a_quick_note_lands_at_the_top_of_another_area_where_the_member_may_write() {
        use crate::ops::areas::{create_area, NewArea};
        use crate::ops::permissions::Role;
        use crate::ops::testing::give_role;
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let engineering = create_area(&core, &grace, NewArea { name: "Engineering".into() }).unwrap().output;
        create_page(&core, &grace, NewEntry { area_id: engineering.id.clone(), parent_id: None, title: "Older".into() }.into())
            .unwrap();
        let into = |area_id: &str| NewQuickNote { title: "Idea".into(), area_id: Some(area_id.into()), ..Default::default() };

        let written = create_quick_note(&core, &grace, into(&engineering.id)).unwrap().output;

        assert_eq!(written.area.name, "Engineering");
        let tree = get_tree(&core, &grace, TreeOfArea { area_id: engineering.id.clone() }).unwrap().output;
        let titles: Vec<_> = tree.entries.iter().map(|entry| entry.title.as_str()).collect();
        assert_eq!(titles, ["Idea", "Older"]);
        assert_eq!(listed(&core, &grace).len(), 1);

        let hidden = create_quick_note(&core, &ken, into(&engineering.id)).unwrap_err();
        assert_eq!((hidden.kind(), hidden.message()), ("not_found", "There is no such Area."));
        give_role(&core, &engineering.id, &ken, Role::Read);
        let refused = create_quick_note(&core, &ken, into(&engineering.id)).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
    }

    #[test]
    fn whoever_has_no_personal_area_within_reach_can_neither_write_nor_list_quick_notes() {
        use crate::ops::areas::{create_area, NewArea};
        use crate::ops::TokenLimits;
        let core = fresh();
        let grace = member(&core, "Grace", false);
        create_quick_note(&core, &grace, quick_note("Mine")).unwrap();
        let engineering = create_area(&core, &grace, NewArea { name: "Engineering".into() }).unwrap().output;
        let Principal::Member { id, .. } = &grace else { unreachable!() };
        let limited = Principal::Member {
            id: id.clone(),
            workspace_admin: false,
            session: None,
            token: Some(TokenLimits { id: "t".into(), read_only: false, areas: Some(vec![engineering.id]) }),
        };
        let agent = Principal::Agent { id: "agent-x".into(), token: TokenLimits { id: "t2".into(), read_only: false, areas: None } };

        let table = [
            (&limited, "This Token does not reach your Personal Area, where Quick Notes land."),
            (&agent, "A standalone Agent has no Personal Area, so it cannot write Quick Notes."),
        ];
        for (principal, message) in table {
            let refused = create_quick_note(&core, principal, quick_note("Idea")).unwrap_err();
            assert_eq!((refused.kind(), refused.message()), ("not_permitted", message), "{principal:?}");
            assert!(listed(&core, principal).is_empty(), "{principal:?}");
        }
        let anonymous = create_quick_note(&core, &Principal::Anonymous, quick_note("Idea")).unwrap_err();
        assert_eq!(anonymous.kind(), "not_permitted");
        assert_eq!(listed(&core, &grace).len(), 1);
    }

    #[test]
    fn a_quick_note_stays_its_members_wherever_it_sits_and_is_listed_only_where_they_read() {
        use crate::ops::areas::{create_area, NewArea};
        use crate::ops::testing::move_to_top_of;
        use crate::ops::tree::{rename_page, RenameEntry};
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let engineering = create_area(&core, &grace, NewArea { name: "Engineering".into() }).unwrap().output;
        let hidden = create_area(&core, &ken, NewArea { name: "Ken's".into() }).unwrap().output;
        let moved = create_quick_note(&core, &grace, quick_note("Moved")).unwrap().output;
        core.advance(60);
        let gone = create_quick_note(&core, &grace, quick_note("Gone")).unwrap().output;
        core.advance(60);
        create_quick_note(&core, &grace, quick_note("Stayed")).unwrap();

        move_to_top_of(&core, &moved.id, &engineering.id);
        move_to_top_of(&core, &gone.id, &hidden.id);
        rename_page(&core, &grace, RenameEntry { id: moved.id.clone(), title: "Moved and renamed".into() }).unwrap();

        let list = list_quick_notes(&core, &grace, RecentQuickNotes { limit: None }).unwrap().output;
        let seen: Vec<_> = list.quick_notes.iter().map(|quick_note| (quick_note.title.as_str(), quick_note.area.name.as_str())).collect();
        assert_eq!(seen, [("Stayed", "Personal"), ("Moved and renamed", "Engineering")]);
        assert!(listed(&core, &ken).is_empty(), "Ken did not write them");
    }

    #[test]
    fn a_limit_lists_more_or_fewer_within_bounds() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        for number in 1..=7 {
            create_quick_note(&core, &grace, quick_note(&format!("Note {number}"))).unwrap();
        }
        let count = |limit| list_quick_notes(&core, &grace, RecentQuickNotes { limit: Some(limit) }).map(|reply| reply.output.quick_notes.len());
        assert_eq!(count(1).unwrap(), 1);
        assert_eq!(count(50).unwrap(), 7);
        for refused in [0, 51] {
            assert_eq!(count(refused).unwrap_err().kind(), "invalid_input", "{refused}");
        }
    }
}
