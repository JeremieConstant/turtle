//! The content of a Page as Markdown, read and written whole, by section or
//! by appending: how Agents work with Pages. Every write changes only what
//! differs, so people editing at the same moment keep their work, and is
//! recorded as a Version of its own at once.

use super::tree::{get_page, page_to_change, PageId};
use super::versions::{version, VersionSummary};
use super::tokens::author_of;
use super::{Catalogue, Core, OpError, OpResult, Principal};
use crate::model::{Block, Inline};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "get_page_markdown",
            "Gets the content of a Page as Markdown. A link to another Page is written as a link to `turtle:` followed by that Page's ID, with the Page's current title as its text, or no text when the caller cannot read that Page. Needs the Read Role.",
            get_page_markdown,
        )
        .changes = false;
    catalogue.add(
        "replace_page",
        "Makes the content of a Page say what the given Markdown says. A link to `turtle:` followed by a Page's ID, as in [Setup](turtle:ID), is a link to that Page; its text is not kept, since the link always shows the Page's current title. A Status, a short coloured marker, is written `:status[ON TRACK]{color=green}`, in grey, blue, green, yellow, red or purple, and grey without a color. Only what differs is changed, so people editing at that moment keep their work, and they see the change at once. Recorded as a Version of its own. Needs the Write Role.",
        replace_page,
    );
    catalogue.add(
        "replace_section",
        "Replaces one Section of a Page with the given Markdown. A Section is a heading and everything up to the next heading of the same or a higher level, so its subsections go with it; the Markdown replaces the heading too, so start it with the heading to keep one. The Section is found by its heading's text, and by its position among Sections with the same heading when there are several. Changed like replace_page, as a Version of its own. Needs the Write Role.",
        replace_section,
    );
    catalogue.add(
        "append_to_page",
        "Adds the given Markdown at the end of a Page, after what is there, which stays as it is. Changed like replace_page, as a Version of its own. Needs the Write Role.",
        append_to_page,
    );
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OfPage {
    pub page_id: String,
}

/// A Page's content as Markdown.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PageMarkdown {
    pub id: String,
    pub title: String,
    pub markdown: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewContent {
    pub page_id: String,
    pub markdown: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewSection {
    pub page_id: String,
    /// The text of the Section's heading, without the leading `#`s, and with a
    /// Page link in it written as its title, as in "See Setup" for
    /// `## See [Setup](turtle:ID)`, and a Status as its text.
    pub heading: String,
    /// Which of the Sections with this heading, counting from 1 at the top
    /// of the Page. Needed only when several Sections have this heading.
    #[serde(default)]
    pub occurrence: Option<u32>,
    /// What replaces the Section, heading included.
    pub markdown: String,
}

/// What a write recorded.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Written {
    /// The Version that holds the write. None when the Page already said that.
    pub version: Option<VersionSummary>,
}

pub fn get_page_markdown(core: &Core, principal: &Principal, input: OfPage) -> OpResult<PageMarkdown> {
    let page = get_page(core, principal, PageId { id: input.page_id })?.output;
    let open = core.live().open(&page.id)?;
    let markdown = super::links::markdown_for(core, principal, &crate::ydoc::read(&open.doc))?;
    Ok(PageMarkdown { id: page.id, title: page.title, markdown }.into())
}

pub fn replace_page(core: &Core, principal: &Principal, input: NewContent) -> OpResult<Written> {
    let blocks = crate::markdown::from_markdown(&input.markdown);
    write(core, principal, &input.page_id, |_| Ok(blocks))
}

/// Replaces the content of a Page the caller may change with `blocks`, as a Version of its own.
pub(super) fn write_content(core: &Core, principal: &Principal, page_id: &str, blocks: Vec<Block>) -> OpResult<Written> {
    write(core, principal, page_id, |_| Ok(blocks))
}

pub fn replace_section(core: &Core, principal: &Principal, input: NewSection) -> OpResult<Written> {
    let replacement = crate::markdown::from_markdown(&input.markdown);
    // A heading is matched as get_page_markdown shows it, with its Page links'
    // titles. They are looked up beforehand, because the change itself runs
    // while the Page is locked; a link added in between counts as untitled.
    page_to_change(core, principal, &input.page_id)?;
    let current = crate::ydoc::read(&core.live().open(&input.page_id)?.doc);
    let titles = super::links::linked_titles(core, principal, &current)?;
    write(core, principal, &input.page_id, |blocks| {
        let range = section(&blocks, &input.heading, input.occurrence, &titles)?;
        let mut blocks = blocks;
        blocks.splice(range, replacement);
        Ok(blocks)
    })
}

pub fn append_to_page(core: &Core, principal: &Principal, input: NewContent) -> OpResult<Written> {
    let appended = crate::markdown::from_markdown(&input.markdown);
    write(core, principal, &input.page_id, |mut blocks| {
        blocks.extend(appended);
        Ok(blocks)
    })
}

/// Where in the content the Section with the given heading is: from its
/// heading to the next heading of the same or a higher level, or the end.
fn section(
    blocks: &[Block],
    heading: &str,
    occurrence: Option<u32>,
    titles: &HashMap<String, String>,
) -> Result<std::ops::Range<usize>, OpError> {
    let heading = heading.trim();
    let headings: Vec<(usize, u8, String)> = blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| match block {
            Block::Heading { level, content } => Some((index, *level, text_of(content, titles))),
            _ => None,
        })
        .collect();
    let matching: Vec<&(usize, u8, String)> = headings.iter().filter(|(_, _, text)| text == heading).collect();
    let chosen = match (matching.len(), occurrence) {
        (0, _) => {
            let known: Vec<String> = headings.iter().map(|(_, _, text)| format!("\u{201c}{text}\u{201d}")).collect();
            let listed = if known.is_empty() {
                "This Page has no headings.".to_string()
            } else {
                format!("The headings are: {}.", known.join(", "))
            };
            return Err(OpError::InvalidInput(format!("No Section is headed \u{201c}{heading}\u{201d}. {listed}")));
        }
        (1, None) => matching[0],
        (count, Some(position)) if (1..=count).contains(&(position as usize)) => matching[position as usize - 1],
        (count, _) => {
            let advice = if count == 1 {
                "Leave out the occurrence, or give 1.".to_string()
            } else {
                format!("Say which with an occurrence from 1 to {count}.")
            };
            let headed = if count == 1 { "1 Section is".to_string() } else { format!("{count} Sections are") };
            return Err(OpError::InvalidInput(format!("{headed} headed \u{201c}{heading}\u{201d}. {advice}")));
        }
    };
    let (start, level, _) = *chosen;
    let end = headings
        .iter()
        .find(|(index, other, _)| *index > start && other <= &level)
        .map_or(blocks.len(), |(index, _, _)| *index);
    Ok(start..end)
}

/// A heading's words without formatting, each Page link as its title.
fn text_of(content: &[Inline], titles: &HashMap<String, String>) -> String {
    let text: String = content
        .iter()
        .map(|inline| match inline {
            Inline::Text { text, .. } => text.as_str(),
            Inline::HardBreak => " ",
            Inline::Image { alt, .. } => alt.as_str(),
            Inline::PageLink { page_id } => titles.get(page_id).map_or("", String::as_str),
            Inline::UnknownDirective { .. } => "",
            Inline::Status { text, .. } => text.as_str(),
        })
        .collect();
    text.trim().to_string()
}

/// Changes a Page the caller may change as `change` makes of its content,
/// as a Version of its own authored by the caller, and returns that Version.
pub(super) fn change_page(
    core: &Core,
    principal: &Principal,
    page_id: &str,
    restored_from: Option<u32>,
    change: impl FnOnce(Vec<Block>) -> Result<Vec<Block>, OpError>,
) -> Result<Option<VersionSummary>, OpError> {
    page_to_change(core, principal, page_id)?;
    let author = author_of(core, principal)?;
    let page = core.live().open(page_id)?;
    let recorded = core.live().change_as_version(&page, &author, restored_from, change)??;
    Ok(match recorded {
        Some(number) => Some(version(&core.db(), page_id, number)?.summary),
        None => None,
    })
}

fn write(
    core: &Core,
    principal: &Principal,
    page_id: &str,
    change: impl FnOnce(Vec<Block>) -> Result<Vec<Block>, OpError>,
) -> OpResult<Written> {
    Ok(Written { version: change_page(core, principal, page_id, None, change)? }.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::testing::{fresh, member};
    use crate::ops::tokens::{create_token, principal_for_token, NewToken};
    use crate::ops::tree::{create_page, NewEntry, NewPage};
    use crate::ops::versions::{list_versions, VersionsOfPage};

    /// An Agent acting through a Token of `owner` called `name`.
    fn agent(core: &Core, owner: &Principal, name: &str) -> Principal {
        let input = NewToken { name: name.into(), read_only: false, area_ids: None, expires_at: None, agent_id: None };
        let value = create_token(core, owner, input).unwrap().output.value;
        principal_for_token(core, &value).unwrap().unwrap()
    }

    /// A Page called Handbook in an Area of its own.
    fn page(core: &Core, writer: &Principal) -> String {
        let area = create_area(core, writer, NewArea { name: "Engineering".into() }).unwrap().output;
        let input = NewEntry { area_id: area.id, parent_id: None, title: "Handbook".into() };
        create_page(core, writer, input.into()).unwrap().output.id
    }

    fn markdown(core: &Core, reader: &Principal, page_id: &str) -> String {
        get_page_markdown(core, reader, OfPage { page_id: page_id.into() }).unwrap().output.markdown
    }

    #[test]
    fn an_agent_replaces_a_page_and_a_version_naming_it_is_recorded_at_once() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let claude = agent(&core, &grace, "Claude");
        let page_id = page(&core, &grace);

        let written = replace_page(&core, &claude, NewContent { page_id: page_id.clone(), markdown: "# Setup\n\nInstall it.".into() })
            .unwrap()
            .output;

        assert_eq!(markdown(&core, &grace, &page_id), "# Setup\n\nInstall it.\n");
        let versions = list_versions(&core, &grace, VersionsOfPage { page_id }).unwrap().output.versions;
        assert_eq!(versions.len(), 1);
        assert_eq!(written.version, Some(versions[0].clone()));
        let claude = crate::ops::versions::Author::AgentForMember {
            name: "Claude".into(),
            member_id: "member-grace".into(),
            member_name: "Grace".into(),
        };
        assert_eq!(versions[0].authors, [claude]);
    }

    fn write_page(core: &Core, writer: &Principal, page_id: &str, markdown: &str) {
        replace_page(core, writer, NewContent { page_id: page_id.into(), markdown: markdown.into() }).unwrap();
    }

    fn section(page_id: &str, heading: &str, occurrence: Option<u32>, markdown: &str) -> NewSection {
        NewSection { page_id: page_id.into(), heading: heading.into(), occurrence, markdown: markdown.into() }
    }

    const GUIDE: &str = "\
# Guide

Welcome.

## Install

Run the installer.

### On Linux

Use the package.

## Use

Open it.
";

    #[test]
    fn a_section_runs_from_its_heading_to_the_next_heading_of_the_same_or_a_higher_level() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let page_id = page(&core, &grace);
        write_page(&core, &grace, &page_id, GUIDE);

        let replaced = section(&page_id, "Install", None, "## Install\n\nDownload it.");
        replace_section(&core, &grace, replaced).unwrap();

        assert_eq!(markdown(&core, &grace, &page_id), "# Guide\n\nWelcome.\n\n## Install\n\nDownload it.\n\n## Use\n\nOpen it.\n");

        // The last section runs to the end, and a heading is matched by its text alone.
        replace_section(&core, &grace, section(&page_id, " Use ", None, "## Use\n\nClick it.")).unwrap();
        assert!(markdown(&core, &grace, &page_id).ends_with("## Use\n\nClick it.\n"));
    }

    #[test]
    fn a_heading_that_is_not_unique_is_told_apart_by_its_position_among_equals() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let page_id = page(&core, &grace);
        write_page(&core, &grace, &page_id, "## Notes\n\nFirst.\n\n## Notes\n\nSecond.\n");

        let ambiguous = replace_section(&core, &grace, section(&page_id, "Notes", None, "## Notes\n\nChanged.")).unwrap_err();
        assert_eq!(ambiguous.kind(), "invalid_input");
        assert_eq!(ambiguous.message(), "2 Sections are headed \u{201c}Notes\u{201d}. Say which with an occurrence from 1 to 2.");
        let beyond = replace_section(&core, &grace, section(&page_id, "Notes", Some(3), "## Notes")).unwrap_err();
        assert_eq!(beyond.message(), "2 Sections are headed \u{201c}Notes\u{201d}. Say which with an occurrence from 1 to 2.");

        replace_section(&core, &grace, section(&page_id, "Notes", Some(2), "## Notes\n\nChanged.")).unwrap();
        assert_eq!(markdown(&core, &grace, &page_id), "## Notes\n\nFirst.\n\n## Notes\n\nChanged.\n");
    }

    #[test]
    fn a_heading_with_a_page_link_is_matched_as_the_caller_sees_it() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let page_id = page(&core, &grace);
        let area_id = crate::ops::tree::get_page(&core, &grace, PageId { id: page_id.clone() }).unwrap().output.area.id;
        let input = NewEntry { area_id, parent_id: None, title: "Setup".into() };
        let setup = create_page(&core, &grace, input.into()).unwrap().output.id;
        write_page(&core, &grace, &page_id, &format!("## Before [x](turtle:{setup})\n\nOld.\n\n## Other\n\nStays."));

        replace_section(&core, &grace, section(&page_id, "Before Setup", None, "## Before\n\nNew.")).unwrap();
        assert_eq!(markdown(&core, &grace, &page_id), "## Before\n\nNew.\n\n## Other\n\nStays.\n");

        write_page(&core, &grace, &page_id, &format!("## [x](turtle:{setup})\n\nOld."));
        let missing = replace_section(&core, &grace, section(&page_id, "Gone", None, "## Gone")).unwrap_err();
        assert_eq!(missing.message(), "No Section is headed \u{201c}Gone\u{201d}. The headings are: \u{201c}Setup\u{201d}.");
    }

    #[test]
    fn a_section_that_does_not_exist_is_invalid_input_naming_the_headings_there_are() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let page_id = page(&core, &grace);
        write_page(&core, &grace, &page_id, GUIDE);

        let missing = replace_section(&core, &grace, section(&page_id, "Uninstall", None, "## Uninstall")).unwrap_err();

        assert_eq!(missing.kind(), "invalid_input");
        assert_eq!(
            missing.message(),
            "No Section is headed \u{201c}Uninstall\u{201d}. The headings are: \u{201c}Guide\u{201d}, \u{201c}Install\u{201d}, \u{201c}On Linux\u{201d}, \u{201c}Use\u{201d}."
        );
        let empty = page(&core, &grace);
        let none = replace_section(&core, &grace, section(&empty, "Install", None, "## Install")).unwrap_err();
        assert_eq!(none.message(), "No Section is headed \u{201c}Install\u{201d}. This Page has no headings.");
        assert_eq!(markdown(&core, &grace, &page_id), GUIDE);
    }

    #[test]
    fn an_agent_appends_to_a_page_and_each_append_is_its_own_version() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let claude = agent(&core, &grace, "Claude");
        let page_id = page(&core, &grace);

        for line in ["Pack the laptop.", "Bring the badge."] {
            append_to_page(&core, &claude, NewContent { page_id: page_id.clone(), markdown: line.into() }).unwrap();
        }

        assert_eq!(markdown(&core, &grace, &page_id), "Pack the laptop.\n\nBring the badge.\n");
        let versions = list_versions(&core, &grace, VersionsOfPage { page_id }).unwrap().output.versions;
        assert_eq!(versions.iter().map(|version| version.number).collect::<Vec<_>>(), [2, 1]);
    }

    #[test]
    fn an_agent_creates_a_page_from_markdown_recorded_as_its_first_version() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let claude = agent(&core, &grace, "Claude");
        let area = create_area(&core, &grace, NewArea { name: "Engineering".into() }).unwrap().output;

        let entry = NewEntry { area_id: area.id, parent_id: None, title: "Runbook".into() };
        let input = NewPage { entry, markdown: Some("# Runbook\n\nRestart it.".into()) };
        let created = create_page(&core, &claude, input).unwrap().output;

        assert_eq!(markdown(&core, &grace, &created.id), "# Runbook\n\nRestart it.\n");
        let versions = list_versions(&core, &grace, VersionsOfPage { page_id: created.id }).unwrap().output.versions;
        assert_eq!(versions.len(), 1);
        let by_claude = matches!(&versions[0].authors[0], crate::ops::versions::Author::AgentForMember { name, .. } if name == "Claude");
        assert!(by_claude, "{:?}", versions[0].authors);
    }

    #[test]
    fn a_persons_edit_made_at_the_same_moment_survives_and_their_earlier_changes_keep_a_version_of_their_own() {
        use yrs::updates::decoder::Decode;
        use yrs::{Doc, GetString, ReadTxn, StateVector, Text, Transact, Update, XmlFragment, XmlOut};
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let claude = agent(&core, &grace, "Claude");
        let page_id = page(&core, &grace);
        write_page(&core, &grace, &page_id, "## Plan\n\nShip it.\n\n## Risks\n\nNone.");
        let open = core.live().open(&page_id).unwrap();
        // Grace changes the Page in her editor; the server has her change...
        let editing = crate::live::Author::member("member-grace");
        core.live().write(&open, &crate::markdown::from_markdown("## Plan\n\nShip it today.\n\n## Risks\n\nNone."), &editing);
        // ...and an editor of hers holds a copy, in which she keeps typing.
        let editor = Doc::new();
        let state = open.doc.transact().encode_state_as_update_v1(&StateVector::default());
        editor.transact_mut().apply_update(Update::decode_v1(&state).unwrap()).unwrap();
        let before = editor.transact().state_vector();
        {
            let fragment = editor.get_or_insert_xml_fragment("default");
            let mut txn = editor.transact_mut();
            let Some(XmlOut::Element(paragraph)) = fragment.get(&txn, 1) else { panic!("no paragraph") };
            let Some(XmlOut::Text(text)) = paragraph.get(&txn, 0) else { panic!("no text") };
            let end = text.get_string(&txn).chars().count() as u32;
            text.insert(&mut txn, end, " Really.");
        }
        let typed = editor.transact().encode_state_as_update_v1(&before);

        replace_section(&core, &claude, section(&page_id, "Risks", None, "## Risks\n\nThe weather.")).unwrap();
        core.live().apply(&open, &typed, &editing).unwrap();

        assert_eq!(markdown(&core, &grace, &page_id), "## Plan\n\nShip it today. Really.\n\n## Risks\n\nThe weather.\n");
        let versions = list_versions(&core, &grace, VersionsOfPage { page_id: page_id.clone() }).unwrap().output.versions;
        let by_agent = |version: &VersionSummary| {
            matches!(&version.authors[..], [crate::ops::versions::Author::AgentForMember { name, .. }] if name == "Claude")
        };
        let listed: Vec<_> = versions.iter().map(|version| (version.number, by_agent(version), version.authors.len())).collect();
        assert_eq!(listed, [(3, true, 1), (2, false, 1), (1, false, 1)]);
    }

    /// The presence entries in the frames sent so far: (client, clock, state).
    fn presence_sent(frames: &mut tokio::sync::broadcast::Receiver<(u64, Vec<u8>)>) -> Vec<(u64, u64, serde_json::Value)> {
        let varuint = |data: &mut &[u8]| crate::live::read_varuint(data).unwrap();
        let mut entries = Vec::new();
        while let Ok((_, frame)) = frames.try_recv() {
            if frame[0] != crate::live::FRAME_PRESENCE {
                continue;
            }
            let mut data = &frame[1..];
            for _ in 0..varuint(&mut data) {
                let (client, clock, len) = (varuint(&mut data), varuint(&mut data), varuint(&mut data) as usize);
                entries.push((client, clock, serde_json::from_slice(&data[..len]).unwrap()));
                data = &data[len..];
            }
        }
        entries
    }

    #[test]
    fn people_with_the_page_open_are_told_which_agent_changed_it_and_see_no_cursor() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let claude = agent(&core, &grace, "Claude");
        let page_id = page(&core, &grace);
        let mut frames = core.live().open(&page_id).unwrap().frames.subscribe();

        append_to_page(&core, &claude, NewContent { page_id: page_id.clone(), markdown: "One.".into() }).unwrap();
        append_to_page(&core, &claude, NewContent { page_id: page_id.clone(), markdown: "Two.".into() }).unwrap();
        // Writing what is already there changes nothing, so nobody is told.
        replace_page(&core, &claude, NewContent { page_id: page_id.clone(), markdown: "One.\n\nTwo.".into() }).unwrap();
        write_page(&core, &grace, &page_id, "Three.");

        let sent = presence_sent(&mut frames);
        assert_eq!(sent.len(), 2, "{sent:?}");
        // An Agent acting for a Member is shown with the Member's name.
        let expected = serde_json::json!({ "user": { "name": "Claude", "agent": true, "member": "Grace" } });
        assert_eq!((sent[0].2.clone(), sent[1].2.clone()), (expected.clone(), expected));
        assert_eq!(sent[0].0, sent[1].0, "one Agent appears once, however often it writes");
        assert!(sent[1].1 > sent[0].1, "a later write is newer to the editors");
    }

    #[test]
    fn an_agents_change_reaches_the_editors_in_a_frame_that_names_the_agent_before_it_is_announced() {
        use yrs::updates::decoder::Decode;
        use yrs::{Doc, Transact, Update};
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let claude = agent(&core, &grace, "Claude");
        let page_id = page(&core, &grace);
        let mut frames = core.live().open(&page_id).unwrap().frames.subscribe();

        append_to_page(&core, &claude, NewContent { page_id: page_id.clone(), markdown: "One.".into() }).unwrap();

        let mut kinds = Vec::new();
        let mut named = None;
        while let Ok((_, frame)) = frames.try_recv() {
            kinds.push(frame[0]);
            if frame[0] == crate::live::FRAME_AGENT_UPDATE {
                let mut data = &frame[1..];
                let len = crate::live::read_varuint(&mut data).unwrap() as usize;
                let who: serde_json::Value = serde_json::from_slice(&data[..len]).unwrap();
                named = Some((who, data[len..].to_vec()));
            }
        }
        // The change comes once, named, and the Agent is announced after it.
        assert_eq!(kinds, [crate::live::FRAME_AGENT_UPDATE, crate::live::FRAME_PRESENCE]);
        let (who, update) = named.unwrap();
        assert_eq!(who, serde_json::json!({ "name": "Claude", "agent": true, "member": "Grace" }));
        let editor = Doc::new();
        editor.transact_mut().apply_update(Update::decode_v1(&update).unwrap()).unwrap();
        assert_eq!(crate::markdown::to_markdown(&crate::ydoc::read(&editor)), "One.\n");
    }

    #[test]
    fn a_refused_write_changes_nothing_and_records_no_version() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let claude = agent(&core, &grace, "Claude");
        let page_id = page(&core, &grace);
        write_page(&core, &grace, &page_id, "## Plan\n\nShip it.");
        let open = core.live().open(&page_id).unwrap();
        let editing = crate::live::Author::member("member-grace");
        core.live().write(&open, &crate::markdown::from_markdown("## Plan\n\nShip it today."), &editing);

        let refused = replace_section(&core, &claude, section(&page_id, "Risks", None, "## Risks"));

        assert_eq!(refused.unwrap_err().kind(), "invalid_input");
        let versions = list_versions(&core, &grace, VersionsOfPage { page_id: page_id.clone() }).unwrap().output.versions;
        assert_eq!(versions.len(), 1, "Grace's change waits for its own Version");
        assert_eq!(markdown(&core, &grace, &page_id), "## Plan\n\nShip it today.\n");
    }
}
