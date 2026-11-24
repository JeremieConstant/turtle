//! PDF Export: an Area or a Subtree as one PDF for reading and handing on,
//! typeset inside Turtle (ADR-0012).

use super::export::{attachment_id, file_name_of, readable_subtree};
use super::tree::{Entry, Kind};
use super::{areas, attachments, fonts, links, pdf_styles};
use super::{Catalogue, Core, OpError, OpResult, Principal};
use crate::directive;
use crate::model::{Block, Inline, Marks};
use crate::pdf;
use base64::Engine;
use schemars::JsonSchema;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "export_pdf",
            "Exports an Area, or the Subtree of one of its Pages or Folders, as one PDF, returned in base64: a cover, a table of contents, then each Page as a chapter in tree order, with Folders as groups. Pages the caller cannot read are left out. It looks as the chosen PDF Style says, or the default one. Needs the Read Role.",
            export_pdf,
        )
        .changes = false;
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ToExportPdf {
    pub area_id: String,
    /// The Page or Folder whose Subtree to export; the whole Area if absent.
    #[serde(default)]
    pub entry_id: Option<String>,
    /// Shown on the cover, beneath the title.
    #[serde(default)]
    pub subtitle: Option<String>,
    /// The PDF Style to make it with, as `list_pdf_styles` names it; the default one if absent.
    #[serde(default)]
    pub pdf_style_id: Option<String>,
}

/// A PDF.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PdfExport {
    /// A name to save it under, after the Area or the top of the Subtree,
    /// such as "Engineering.pdf".
    pub file_name: String,
    /// The PDF, in base64.
    pub content: String,
}

pub fn export_pdf(core: &Core, principal: &Principal, input: ToExportPdf) -> OpResult<PdfExport> {
    areas::signed_in(principal)?;
    // What is in the PDF is decided under one lock, so the tree is seen as
    // it was at one moment; the content is read afterwards, as each Page is now.
    let (name, entries, look, fonts, logo) = {
        let db = core.db();
        let (name, entries) = readable_subtree(&db, principal, &input.area_id, input.entry_id.as_deref())?;
        let style = pdf_styles::pdf_style(&db, input.pdf_style_id.as_deref())?;
        let logo = pdf_styles::stored_logo(&db, &style.id)?;
        let look = pdf_styles::look_of(style.values);
        let families = [look.body_font.as_str(), look.heading_font.as_str(), look.code_font.as_str()];
        let fonts = fonts::files_of_families(core, &db, &families)?;
        (name, entries, look, fonts, logo)
    };
    let logo = pdf_styles::logo_for_pdf(core, logo);
    let mut flat = Vec::new();
    flatten(&entries, 0, &mut flat);
    let mut contents = HashMap::new();
    for (entry, _) in flat.iter().filter(|(entry, _)| entry.kind == Kind::Page) {
        contents.insert(entry.id.clone(), core.live().content(&entry.id)?);
    }
    let all_blocks: Vec<Block> = contents.values().flatten().cloned().collect();

    let in_pdf = flat
        .iter()
        .filter(|(entry, _)| entry.kind == Kind::Page)
        .map(|(entry, _)| (entry.id.clone(), entry.title.clone()))
        .collect();
    let mut found = Found {
        in_pdf,
        outside: links::linked_titles(core, principal, &all_blocks)?,
        names: links::mentioned_names(core, &all_blocks)?,
        public_url: core.public_url().map(|url| url.as_str().to_string()),
        attachment_names: HashMap::new(),
        images: HashMap::new(),
    };
    find_attachments(core, principal, &all_blocks, &mut found)?;

    let parts = flat
        .iter()
        .map(|(entry, depth)| match entry.kind {
            Kind::Folder => pdf::Part {
                kind: pdf::PartKind::Folder,
                label: folder_label(&entry.id),
                title: entry.title.clone(),
                depth: *depth,
                sections: Vec::new(),
                blocks: Vec::new(),
            },
            Kind::Page => {
                let mut chapter = Chapter { page_id: &entry.id, found: &found, sections: Vec::new() };
                let blocks = chapter.blocks(&contents[&entry.id], 0);
                pdf::Part {
                    kind: pdf::PartKind::Page,
                    label: page_label(&entry.id),
                    title: entry.title.clone(),
                    depth: *depth,
                    sections: chapter.sections,
                    blocks,
                }
            }
        })
        .collect();
    let document = pdf::Document {
        title: name.clone(),
        subtitle: input.subtitle.map(|subtitle| subtitle.trim().to_string()).filter(|subtitle| !subtitle.is_empty()),
        as_of: pdf::Date::of(core.now()),
        parts,
        images: found.images,
        logo,
        fonts,
    };
    let pdf = pdf::typeset(document, &look)?;
    Ok(PdfExport {
        file_name: format!("{}.pdf", file_name_of(&name)),
        content: base64::engine::general_purpose::STANDARD.encode(pdf),
    }
    .into())
}

/// The entries in tree order, each with how many Folders above it are in the PDF.
fn flatten<'a>(entries: &'a [Entry], depth: usize, out: &mut Vec<(&'a Entry, usize)>) {
    for entry in entries {
        out.push((entry, depth));
        let beneath = if entry.kind == Kind::Folder { depth + 1 } else { depth };
        flatten(&entry.children, beneath, out);
    }
}

fn page_label(page_id: &str) -> String {
    format!("p-{page_id}")
}

fn folder_label(folder_id: &str) -> String {
    format!("f-{folder_id}")
}

/// What the content refers to, as the exporter may see it.
struct Found {
    /// The Pages in the PDF, with their titles.
    in_pdf: HashMap<String, String>,
    /// The titles of other Pages the content links to that the exporter can read.
    outside: HashMap<String, String>,
    public_url: Option<String>,
    /// The current names of the people the content mentions.
    names: HashMap<String, String>,
    /// The names of the Attachments the content refers to that the exporter can read.
    attachment_names: HashMap<String, String>,
    /// Of those, the images the PDF can show, by their path in the document.
    images: HashMap<String, Vec<u8>>,
}

fn image_path(attachment_id: &str) -> String {
    format!("/images/{attachment_id}")
}

/// Reads the Attachments the content shows as images, keeping those the
/// exporter can read and the PDF can show, and the names of all it refers to.
fn find_attachments(core: &Core, principal: &Principal, blocks: &[Block], found: &mut Found) -> Result<(), OpError> {
    let mut referred = Vec::new();
    let mut shown = Vec::new();
    for_each_inline(blocks, &mut |inline| match inline {
        Inline::Image { src, .. } => shown.extend(attachment_id(src).map(String::from)),
        Inline::Text { marks: Marks { link: Some(href), .. }, .. } => {
            referred.extend(attachment_id(href).map(String::from))
        }
        Inline::Text { .. }
        | Inline::HardBreak
        | Inline::PageLink { .. }
        | Inline::Mention { .. }
        | Inline::UnknownDirective { .. }
        | Inline::Status { .. } => {}
    });
    let mut files = Vec::new();
    {
        let db = core.db();
        for id in referred.iter().chain(&shown) {
            if found.attachment_names.contains_key(id) {
                continue;
            }
            if let Some((attachment, content_hash)) = attachments::readable_stored(&db, principal, id)? {
                found.attachment_names.insert(id.clone(), attachment.name);
                if shown.contains(id) {
                    files.push((id.clone(), content_hash));
                }
            }
        }
    }
    for (id, content_hash) in files {
        let stored = core.attachments_dir().join(&content_hash);
        // A file gone since, or one the PDF cannot show, is shown by name.
        match std::fs::read(&stored) {
            Ok(content) if pdf::is_image(&content) => {
                found.images.insert(image_path(&id), content);
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(anyhow::Error::from(error).context(format!("reading {}", stored.display())).into()),
        }
    }
    Ok(())
}

/// From this depth on, lists, quotes and Callouts are shown as their
/// content: the typesetter refuses to nest without end.
const DEEPEST: usize = 8;

/// One Page's content, made into a chapter.
struct Chapter<'a> {
    page_id: &'a str,
    found: &'a Found,
    sections: Vec<pdf::Section>,
}

impl Chapter<'_> {
    fn blocks(&mut self, blocks: &[Block], depth: usize) -> Vec<pdf::Block> {
        let mut out = Vec::new();
        for block in blocks {
            self.block(block, depth, &mut out);
        }
        out
    }

    /// The content of a quote or Callout `depth` deep; from `DEEPEST` on,
    /// it is put in the quote's place, in `out`.
    fn nested(&mut self, children: &[Block], depth: usize, out: &mut Vec<pdf::Block>) -> Option<Vec<pdf::Block>> {
        if depth >= DEEPEST {
            children.iter().for_each(|child| self.block(child, depth, out));
            None
        } else {
            Some(self.blocks(children, depth + 1))
        }
    }

    /// The items of a list `depth` deep, as `nested`.
    fn items(&mut self, items: &[Vec<Block>], depth: usize, out: &mut Vec<pdf::Block>) -> Option<Vec<Vec<pdf::Block>>> {
        if depth >= DEEPEST {
            items.iter().flatten().for_each(|child| self.block(child, depth, out));
            None
        } else {
            Some(items.iter().map(|item| self.blocks(item, depth + 1)).collect())
        }
    }

    fn block(&mut self, block: &Block, depth: usize, out: &mut Vec<pdf::Block>) {
        let shown = match block {
            Block::Paragraph(content) => pdf::Block::Paragraph { inlines: self.inlines(content) },
            Block::Heading { level, content } => {
                // The chapter's title is the one heading above a Page's own.
                let rank = (*level).clamp(2, 6);
                let inlines = self.inlines(content);
                let title = plain_text(&inlines);
                let label = (!title.is_empty()).then(|| format!("s-{}-{}", self.page_id, self.sections.len() + 1));
                if let Some(label) = &label {
                    self.sections.push(pdf::Section { title, label: label.clone(), rank });
                }
                pdf::Block::Heading { rank, label, inlines }
            }
            Block::CodeBlock { language, text } => pdf::Block::Code {
                language: language.clone().filter(|language| !language.trim().is_empty()),
                text: text.clone(),
            },
            Block::Blockquote(children) => match self.nested(children, depth, out) {
                Some(blocks) => pdf::Block::Quote { blocks },
                None => return,
            },
            Block::Callout { kind, content } => match self.nested(content, depth, out) {
                Some(blocks) => pdf::Block::Callout { kind: kind.name(), blocks },
                None => return,
            },
            Block::BulletList(items) => match self.items(items, depth, out) {
                Some(items) => pdf::Block::Bullets { items },
                None => return,
            },
            Block::OrderedList { start, items } => match self.items(items, depth, out) {
                Some(items) => pdf::Block::Numbered { start: u32::try_from(*start).unwrap_or(u32::MAX), items },
                None => return,
            },
            Block::TaskList(tasks) => {
                if depth >= DEEPEST {
                    tasks.iter().flat_map(|task| &task.content).for_each(|child| self.block(child, depth, out));
                    return;
                }
                let items = tasks
                    .iter()
                    .map(|task| pdf::Task { checked: task.checked, blocks: self.blocks(&task.content, depth + 1) })
                    .collect();
                pdf::Block::Tasks { items }
            }
            Block::Table(rows) => {
                let width = rows.first().map_or(0, Vec::len);
                if width == 0 {
                    return;
                }
                let rows = rows
                    .iter()
                    .map(|row| {
                        let mut cells: Vec<_> = row.iter().take(width).map(|cell| self.inlines(cell)).collect();
                        cells.resize_with(width, Vec::new);
                        cells
                    })
                    .collect();
                pdf::Block::Table { rows }
            }
            Block::HorizontalRule => pdf::Block::Rule,
            Block::UnknownDirective { source } => pdf::Block::Unknown { name: directive_name(source) },
        };
        out.push(shown);
    }

    fn inlines(&self, inlines: &[Inline]) -> Vec<pdf::Inline> {
        inlines.iter().filter_map(|inline| self.inline(inline)).collect()
    }

    fn inline(&self, inline: &Inline) -> Option<pdf::Inline> {
        let found = self.found;
        match inline {
            Inline::Text { text, marks } => {
                // An Attachment that is not shown is named only: the PDF does not carry it.
                let link = marks.link.as_deref().filter(|href| attachment_id(href).is_none()).and_then(outside_link);
                Some(pdf::Inline::Text {
                    text: text.clone(),
                    bold: marks.bold,
                    italic: marks.italic,
                    strike: marks.strike,
                    code: marks.code,
                    link,
                })
            }
            Inline::HardBreak => Some(pdf::Inline::Break),
            Inline::Image { src, alt } => match attachment_id(src) {
                Some(id) => {
                    let path = image_path(id);
                    if found.images.contains_key(&path) {
                        Some(pdf::Inline::Image { path, alt: alt.clone() })
                    } else {
                        let name = found.attachment_names.get(id).unwrap_or(alt);
                        (!name.is_empty()).then(|| plain(name, None))
                    }
                }
                // The PDF holds no images from elsewhere, only a link to them.
                None => {
                    let shown = if alt.is_empty() { src } else { alt };
                    (!shown.is_empty()).then(|| plain(shown, outside_link(src)))
                }
            },
            Inline::PageLink { page_id } => {
                if let Some(title) = found.in_pdf.get(page_id) {
                    return Some(plain(title, Some(pdf::Link::Part(page_label(page_id)))));
                }
                // Nothing to someone who cannot read the Page, as everywhere.
                let title = found.outside.get(page_id)?;
                // Without its public URL, Turtle cannot say where the Page is.
                let link = found.public_url.as_ref().map(|base| pdf::Link::Url(format!("{base}/p/{page_id}")));
                Some(plain(title, link))
            }
            // A badge with the name alone: the PDF shows no pictures of
            // people and leads to no Profile (ADR-0012).
            Inline::Mention { person_id } => {
                found.names.get(person_id).map(|name| pdf::Inline::Mention { name: name.clone() })
            }
            Inline::UnknownDirective { source } => Some(pdf::Inline::Unknown { name: directive_name(source) }),
            // Shown in uppercase, as in the app.
            Inline::Status { text, color } => {
                (!text.trim().is_empty()).then(|| pdf::Inline::Status { text: text.to_uppercase(), color: color.name() })
            }
        }
    }
}

/// The name a directive Turtle does not know is shown by, as in the editor.
fn directive_name(source: &str) -> String {
    directive::name_of(source).unwrap_or_default().to_string()
}

fn plain(text: &str, link: Option<pdf::Link>) -> pdf::Inline {
    pdf::Inline::Text { text: text.to_string(), bold: false, italic: false, strike: false, code: false, link }
}

/// A link out of the PDF, for an address its reader can follow.
fn outside_link(href: &str) -> Option<pdf::Link> {
    let lower = href.to_ascii_lowercase();
    ["https://", "http://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
        .then(|| pdf::Link::Url(href.to_string()))
}

fn plain_text(inlines: &[pdf::Inline]) -> String {
    let text: String = inlines
        .iter()
        .map(|inline| match inline {
            pdf::Inline::Text { text, .. } | pdf::Inline::Status { text, .. } => text.as_str(),
            pdf::Inline::Break => " ",
            pdf::Inline::Image { .. } | pdf::Inline::Unknown { .. } => "",
            pdf::Inline::Mention { name } => name.as_str(),
        })
        .collect();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn for_each_inline(blocks: &[Block], visit: &mut impl FnMut(&Inline)) {
    for block in blocks {
        match block {
            Block::Paragraph(content) | Block::Heading { content, .. } => content.iter().for_each(&mut *visit),
            Block::Blockquote(children) | Block::Callout { content: children, .. } => for_each_inline(children, visit),
            Block::BulletList(items) | Block::OrderedList { items, .. } => {
                items.iter().for_each(|item| for_each_inline(item, visit))
            }
            Block::TaskList(items) => items.iter().for_each(|item| for_each_inline(&item.content, visit)),
            Block::Table(rows) => rows.iter().flatten().flatten().for_each(&mut *visit),
            Block::CodeBlock { .. } | Block::HorizontalRule | Block::UnknownDirective { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::attachments::{attach_file, NewAttachment};
    use crate::ops::content::{replace_page, NewContent};
    use crate::ops::permissions::{self, Holder, Role};
    use crate::ops::testing::{fresh, give_role, member};
    use crate::ops::accounts::Nothing;
    use crate::ops::pdf_styles::{
        create_pdf_style, list_pdf_styles, remove_pdf_style_logo, set_default_pdf_style, set_pdf_style_logo, NewPdfStyleLogo,
        OfPdfStyle, PdfStyleId, PdfStyleValues,
    };
    use crate::ops::tree::{create_folder, create_page, NewEntry, NewPage};
    use crate::pdf::Language;
    use std::collections::BTreeSet;
    use base64::Engine;

    fn area(core: &Core, admin: &Principal, name: &str) -> String {
        create_area(core, admin, NewArea { name: name.into() }).unwrap().output.id
    }

    /// Creates a Page with content, beneath `parent` or at the top of the Area.
    fn page(core: &Core, writer: &Principal, area_id: &str, parent: Option<&str>, title: &str, markdown: &str) -> String {
        let entry = NewEntry { area_id: area_id.into(), parent_id: parent.map(Into::into), title: title.into() };
        let markdown = (!markdown.is_empty()).then(|| markdown.to_string());
        create_page(core, writer, NewPage { entry, markdown }).unwrap().output.id
    }

    fn pdf_of(export: &PdfExport) -> pdf_extract::Document {
        let bytes = base64::engine::general_purpose::STANDARD.decode(&export.content).unwrap();
        pdf_extract::Document::load_mem(&bytes).unwrap()
    }

    /// A PDF's pages, each as the text it shows, as `squashed` gives it.
    fn pages_of(export: &PdfExport) -> Vec<String> {
        let bytes = base64::engine::general_purpose::STANDARD.decode(&export.content).unwrap();
        pdf_extract::extract_text_from_mem_by_pages(&bytes).unwrap().iter().map(|text| squashed(text)).collect()
    }

    /// Text without its spaces and line breaks, which reading a PDF puts
    /// where the letters stand apart rather than where words end, and
    /// without the dots that lead to a page number.
    fn squashed(text: &str) -> String {
        let mut text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        while let Some(at) = text.find("...") {
            let end = text[at..].find(|c| c != '.').map_or(text.len(), |length| at + length);
            text.replace_range(at..end, "");
        }
        text
    }

    fn export_area(core: &Core, reader: &Principal, area_id: &str) -> PdfExport {
        let input = ToExportPdf { area_id: area_id.into(), entry_id: None, subtitle: None, pdf_style_id: None };
        export_pdf(core, reader, input).unwrap().output
    }

    #[test]
    fn an_area_is_exported_as_a_pdf_with_a_cover_contents_and_a_chapter_per_page() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        page(&core, &grace, &area_id, None, "Runbook", "Restart the server.");
        page(&core, &grace, &area_id, None, "Deploy", "Ship it.");

        let exported = export_area(&core, &grace, &area_id);
        let pages = pages_of(&exported);

        assert_eq!(exported.file_name, "Engineering.pdf");
        assert_eq!(pages.len(), 4, "{pages:#?}");
        // The cover has no running header and no page number.
        assert_eq!(pages[0], squashed("Engineering AS OF 14 November 2023"));
        // Every other page has the title above and its number below.
        assert_eq!(pages[1], squashed("ENGINEERING Contents Runbook 3 Deploy 4 2"));
        assert_eq!(pages[2], squashed("ENGINEERING Runbook Restart the server. 3"));
        assert_eq!(pages[3], squashed("ENGINEERING Deploy Ship it. 4"));
    }

    fn folder(core: &Core, writer: &Principal, area_id: &str, parent: Option<&str>, title: &str) -> String {
        let entry = NewEntry { area_id: area_id.into(), parent_id: parent.map(Into::into), title: title.into() };
        create_folder(core, writer, entry).unwrap().output.id
    }

    #[test]
    fn folders_are_groups_and_the_contents_list_each_pages_sections() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let guides = folder(&core, &grace, &area_id, None, "Guides");
        page(&core, &grace, &area_id, Some(&guides), "Set up", "## Install\n\nRun it.\n\n### Details\n\nMore.\n\n## Check");
        page(&core, &grace, &area_id, None, "Runbook", "# Restart\n\nRestart the server.");

        let pages = pages_of(&export_area(&core, &grace, &area_id));

        // `###` Sections are left out of the contents, at depth 2.
        assert_eq!(pages[1], squashed("ENGINEERING Contents GUIDES Set up 3 Install 3 Check 3 Runbook 4 Restart 4 2"));
        // A Folder's heading starts the page of what is beneath it.
        assert_eq!(pages[2], squashed("ENGINEERING GUIDES Set up Install Run it. Details More. Check 3"));
        assert_eq!(pages[3], squashed("ENGINEERING Runbook Restart Restart the server. 4"));
    }

    #[test]
    fn a_subtree_is_exported_alone_with_its_subtitle_on_the_cover() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        page(&core, &grace, &area_id, None, "Overview", "Elsewhere.");
        let runbook = page(&core, &grace, &area_id, None, "Runbook", "Restart the server.");
        page(&core, &grace, &area_id, Some(&runbook), "Deploy", "Ship it.");

        let input = ToExportPdf { area_id, entry_id: Some(runbook), subtitle: Some("For whoever is on call".into()), pdf_style_id: None };
        let exported = export_pdf(&core, &grace, input).unwrap().output;
        let pages = pages_of(&exported);

        assert_eq!(exported.file_name, "Runbook.pdf");
        assert_eq!(
            pages,
            [
                "Runbook For whoever is on call AS OF 14 November 2023",
                "RUNBOOK Contents Runbook 3 Deploy 4 2",
                "RUNBOOK Runbook Restart the server. 3",
                "RUNBOOK Deploy Ship it. 4",
            ]
            .map(squashed)
        );
    }

    #[test]
    fn typst_syntax_in_titles_and_content_comes_out_as_written() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let written = r#"#panic("no") $x$ [a] \ @ref <label> *b* _i_ #set page(fill: red)"#;
        let area_id = area(&core, &grace, written);
        let markdown = r#"`#panic("no")` \#panic("no") $x$ \[a\] \\ @ref \<label\> \*b\* \_i\_"#;
        page(&core, &grace, &area_id, None, written, markdown);

        let pages = pages_of(&export_area(&core, &grace, &area_id));

        assert_eq!(pages[0], squashed(&format!("{written} AS OF 14 November 2023")));
        let content = r#"#panic("no") #panic("no") $x$ [a] \ @ref <label> *b* _i_"#;
        assert_eq!(pages[2], squashed(&format!("{} {written} {content} 3", written.to_uppercase())));
    }

    #[test]
    fn every_block_and_inline_the_editor_holds_is_shown() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let markdown = "Some **bold**, *italic*, ~~struck~~ and `code`,  \nthen a [link](https://example.org).\n\n\
            - one\n  - nested\n\n3. third\n4. fourth\n\n- [x] done\n- [ ] open\n\n> quoted\n\n> [!WARNING]\n> Careful.\n\n\
            | Name | Role |\n| --- | --- |\n| Grace | Admin |\n\n```rust\nfn main() {}\n```\n\n---\n\nThe end.";
        page(&core, &grace, &area_id, None, "Everything", markdown);

        let pages = pages_of(&export_area(&core, &grace, &area_id));

        assert_eq!(
            pages[2],
            squashed(
                "ENGINEERING Everything Some bold, italic, struck and code, then a link. – one – nested 3. third 4. fourth \
                 done open quoted WARNING Careful. NAME ROLE Grace Admin fn main() {} The end. 3"
            )
        );
    }

    #[test]
    fn a_directive_turtle_does_not_know_is_shown_as_a_placeholder_naming_it() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let markdown = "Plan :acme.demo[hi]{x=1} today.\n\n:::acme.board{x=1}\n- a\n:::\n";
        page(&core, &grace, &area_id, None, "Board", markdown);

        let pages = pages_of(&export_area(&core, &grace, &area_id));

        assert_eq!(
            pages[2],
            squashed("ENGINEERING Board Plan Unknown element: acme.demo today. Unknown element: acme.board 3")
        );
    }

    #[test]
    fn a_status_is_shown_as_a_marker_in_uppercase() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let markdown = "## Build :status[on track]{color=green}\n\n| Task | State |\n| - | - |\n| Ship | :status[late] |\n";
        page(&core, &grace, &area_id, None, "Board", markdown);

        let pages = pages_of(&export_area(&core, &grace, &area_id));

        // The contents name the Section by what it shows.
        assert!(pages[1].contains(&squashed("Build ON TRACK")), "{}", pages[1]);
        assert_eq!(pages[2], squashed("ENGINEERING Board Build ON TRACK TASK STATE Ship LATE 3"));
    }

    /// A PNG of one pixel.
    const PIXEL: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    fn attach(core: &Core, writer: &Principal, page_id: &str, name: &str, content: &str) -> String {
        let input = NewAttachment { page_id: page_id.into(), name: name.into(), content: content.into() };
        attach_file(core, writer, input).unwrap().output.id
    }

    /// How many images the PDF shows. An image's transparency is an image
    /// of its own, which is not counted.
    fn images_in(export: &PdfExport) -> usize {
        let document = pdf_of(export);
        let images: Vec<&pdf_extract::Stream> = document
            .objects
            .values()
            .filter_map(|object| object.as_stream().ok())
            .filter(|stream| stream.dict.get(b"Subtype").and_then(|subtype| subtype.as_name()).ok() == Some(b"Image"))
            .collect();
        let masks = images.iter().filter(|image| image.dict.has(b"SMask")).count();
        images.len() - masks
    }

    #[test]
    fn image_attachments_are_embedded_and_others_named() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let runbook = page(&core, &grace, &area_id, None, "Runbook", "");
        let pixel = attach(&core, &grace, &runbook, "pixel.png", PIXEL);
        let broken = base64::engine::general_purpose::STANDARD.encode("not an image");
        let broken = attach(&core, &grace, &runbook, "broken.png", &broken);
        let log = attach(&core, &grace, &runbook, "deploy.log", &base64::engine::general_purpose::STANDARD.encode("a log"));
        let markdown = format!(
            "![A pixel](/api/attachments/{pixel})\n\n![Broken](/api/attachments/{broken})\n\nSee [the log](/api/attachments/{log})."
        );
        write(&core, &grace, &runbook, &markdown);

        let exported = export_area(&core, &grace, &area_id);

        assert_eq!(images_in(&exported), 1);
        assert_eq!(pages_of(&exported)[2], squashed("ENGINEERING Runbook broken.png See the log. 3"));
        // Only the contents link, to the chapter.
        assert_eq!(link_targets(&exported), ["page 3"]);
    }

    fn write(core: &Core, writer: &Principal, page_id: &str, markdown: &str) {
        replace_page(core, writer, NewContent { page_id: page_id.into(), markdown: markdown.into() }).unwrap();
    }

    /// Where the PDF's links lead, each once: an address, or "page N" for a page of the PDF.
    fn link_targets(export: &PdfExport) -> Vec<String> {
        let document = pdf_of(export);
        let pages: HashMap<pdf_extract::ObjectId, u32> = document.get_pages().into_iter().map(|(number, id)| (id, number)).collect();
        let named = named_destinations(&document);
        let mut targets = Vec::new();
        for page_id in pages.keys() {
            for annotation in document.get_page_annotations(*page_id).unwrap() {
                if annotation.get(b"Subtype").and_then(|subtype| subtype.as_name()).ok() != Some(b"Link") {
                    continue;
                }
                let action = annotation.get(b"A").and_then(|action| document.dereference(action)).map(|(_, action)| action);
                let uri = action.ok().and_then(|action| action.as_dict().ok()).and_then(|action| action.get(b"URI").ok());
                let destination = annotation.get(b"Dest").ok();
                if let Some(uri) = uri {
                    targets.push(String::from_utf8(uri.as_str().unwrap().to_vec()).unwrap());
                } else if let Some(destination) = destination {
                    let (_, mut destination) = document.dereference(destination).unwrap();
                    if let Ok(name) = destination.as_str() {
                        destination = named[name];
                    }
                    let page = destination.as_array().unwrap()[0].as_reference().unwrap();
                    targets.push(format!("page {}", pages[&page]));
                }
            }
        }
        targets.sort();
        // A link that breaks across pieces of text is one annotation per piece.
        targets.dedup();
        targets
    }

    /// The destinations the PDF names, by name.
    fn named_destinations(document: &pdf_extract::Document) -> HashMap<Vec<u8>, &pdf_extract::Object> {
        let mut named = HashMap::new();
        let catalog = document.catalog().unwrap();
        let Ok(names) = catalog.get(b"Names").and_then(|names| document.dereference(names)) else { return named };
        let dests = names.1.as_dict().unwrap().get(b"Dests").and_then(|dests| document.dereference(dests)).unwrap().1;
        let mut nodes = vec![dests.as_dict().unwrap()];
        while let Some(node) = nodes.pop() {
            if let Ok(kids) = node.get(b"Kids").and_then(|kids| kids.as_array()) {
                for kid in kids {
                    nodes.push(document.dereference(kid).unwrap().1.as_dict().unwrap());
                }
            }
            if let Ok(pairs) = node.get(b"Names").and_then(|pairs| pairs.as_array()) {
                for pair in pairs.chunks(2) {
                    let destination = document.dereference(&pair[1]).unwrap().1;
                    named.insert(pair[0].as_str().unwrap().to_vec(), destination);
                }
            }
        }
        named
    }

    #[test]
    fn page_links_into_the_pdf_lead_to_their_chapter_and_others_to_the_page_in_turtle() {
        let mut core = fresh();
        core.core.set_public_url(Some(crate::public_url::PublicUrl::parse("https://turtle.example.org").unwrap()));
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let overview = page(&core, &grace, &area_id, None, "Overview", "");
        let runbook = page(&core, &grace, &area_id, None, "Runbook", "");
        let deploy = page(&core, &grace, &area_id, Some(&runbook), "Deploy", "");
        write(&core, &grace, &runbook, &format!("See [](turtle:{overview}) and [](turtle:{deploy})."));

        let input = ToExportPdf { area_id, entry_id: Some(runbook), subtitle: None, pdf_style_id: None };
        let exported = export_pdf(&core, &grace, input).unwrap().output;

        assert_eq!(pages_of(&exported)[2], squashed("RUNBOOK Runbook See Overview and Deploy. 3"));
        // The contents lead to the chapter too.
        assert_eq!(
            link_targets(&exported),
            [format!("https://turtle.example.org/p/{overview}").as_str(), "page 3", "page 4"]
        );
    }

    #[test]
    fn a_mention_is_a_badge_with_the_current_name_and_no_link() {
        let mut core = fresh();
        core.core.set_public_url(Some(crate::public_url::PublicUrl::parse("https://turtle.example.org").unwrap()));
        let grace = member(&core, "Grace", false);
        member(&core, "Ken", false);
        let area_id = area(&core, &grace, "Engineering");
        page(&core, &grace, &area_id, None, "Runbook", "Ask [@Kenneth](turtle:member/member-ken) or [@x](turtle:member/nobody).");

        let exported = export_area(&core, &grace, &area_id);

        assert_eq!(pages_of(&exported)[2], squashed("ENGINEERING Runbook Ask Ken or . 3"));
        assert!(link_targets(&exported).iter().all(|target| target.starts_with("page ")));
        assert_eq!(images_in(&exported), 0);
    }

    #[test]
    fn without_a_public_url_a_link_to_a_page_outside_the_pdf_is_its_title_alone() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let overview = page(&core, &grace, &area_id, None, "Overview", "");
        let runbook = page(&core, &grace, &area_id, None, "Runbook", &format!("See [](turtle:{overview})."));

        let input = ToExportPdf { area_id, entry_id: Some(runbook), subtitle: None, pdf_style_id: None };
        let exported = export_pdf(&core, &grace, input).unwrap().output;

        assert_eq!(pages_of(&exported)[2], squashed("RUNBOOK Runbook See Overview. 3"));
        assert_eq!(link_targets(&exported), ["page 3"]);
    }

    #[test]
    fn pages_the_exporter_cannot_read_are_left_out_and_what_they_can_read_beneath_takes_their_place() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area_id = area(&core, &grace, "Engineering");
        give_role(&core, &area_id, &ken, Role::Read);
        let runbook = page(&core, &grace, &area_id, None, "Runbook", "");
        let secrets = page(&core, &grace, &area_id, Some(&runbook), "Secrets", "The password.");
        page(&core, &grace, &area_id, Some(&secrets), "Rotation", "Rotate monthly.");
        write(&core, &grace, &runbook, &format!("Keep [](turtle:{secrets}) safe."));
        // Secrets has Grants of its own, which leave Ken out; Rotation, beneath it, lets him read again.
        let Principal::Member { id: grace_id, .. } = &grace else { unreachable!() };
        let db = core.db();
        permissions::grant(&db, &secrets, &Holder::Member(grace_id.clone()), Role::Admin).unwrap();
        drop(db);
        let rotation = core.db().query_row("SELECT id FROM nodes WHERE title = 'Rotation'", [], |row| row.get::<_, String>(0)).unwrap();
        let Principal::Member { id: ken_id, .. } = &ken else { unreachable!() };
        permissions::grant(&core.db(), &rotation, &Holder::Member(ken_id.clone()), Role::Read).unwrap();

        let pages = pages_of(&export_area(&core, &ken, &area_id));

        assert_eq!(
            pages[1..],
            ["ENGINEERING Contents Runbook 3 Rotation 4 2", "ENGINEERING Runbook Keep safe. 3", "ENGINEERING Rotation Rotate monthly. 4"]
                .map(squashed)
        );
        let hidden = ToExportPdf { area_id: area_id.clone(), entry_id: Some(secrets), subtitle: None, pdf_style_id: None };
        let hidden = export_pdf(&core, &ken, hidden).unwrap_err();
        let made_up = ToExportPdf { area_id, entry_id: Some("made-up".into()), subtitle: None, pdf_style_id: None };
        let made_up = export_pdf(&core, &ken, made_up).unwrap_err();
        assert_eq!((hidden.kind(), hidden.message()), (made_up.kind(), made_up.message()));
        assert_eq!(hidden.kind(), "not_found");
    }

    #[test]
    fn content_nested_deeper_than_the_typesetter_goes_is_still_shown() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let mut markdown = String::new();
        for depth in 0..60 {
            markdown.push_str(&format!("{}- level {depth}\n", "  ".repeat(depth)));
        }
        markdown.push_str(&format!("\n{}deep quote", "> ".repeat(60)));
        page(&core, &grace, &area_id, None, "Deep", &markdown);

        let pages = pages_of(&export_area(&core, &grace, &area_id));

        let text = pages[2..].join("");
        assert!(text.contains("level0") && text.contains("level59") && text.contains("deepquote"), "{text}");
    }

    /// Every drawing operation on the PDF's pages, with its numbers.
    fn operations_in(export: &PdfExport) -> Vec<(String, Vec<f32>)> {
        let document = pdf_of(export);
        let mut operations = Vec::new();
        for page_id in document.get_pages().into_values() {
            let content = pdf_extract::content::Content::decode(&document.get_page_content(page_id).unwrap()).unwrap();
            for operation in content.operations {
                let numbers = operation.operands.iter().filter_map(|operand| operand.as_float().ok()).collect();
                operations.push((operation.operator, numbers));
            }
        }
        operations
    }

    /// The colours the PDF paints with, as `#rrggbb`.
    fn colors_in(export: &PdfExport) -> BTreeSet<String> {
        let channel = |value: f32| (value * 255.0).round() as u8;
        operations_in(export)
            .into_iter()
            .filter(|(operator, numbers)| ["scn", "SCN"].contains(&operator.as_str()) && numbers.len() == 3)
            .map(|(_, rgb)| format!("#{:02x}{:02x}{:02x}", channel(rgb[0]), channel(rgb[1]), channel(rgb[2])))
            .collect()
    }

    /// The sizes of the PDF's text, in points.
    fn text_sizes_in(export: &PdfExport) -> BTreeSet<String> {
        operations_in(export)
            .into_iter()
            .filter(|(operator, numbers)| operator == "Tf" && numbers.len() == 1)
            .map(|(_, size)| size[0].to_string())
            .collect()
    }

    /// The font families the PDF's text is in, as the fonts name them.
    fn fonts_in(export: &PdfExport) -> BTreeSet<String> {
        let document = pdf_of(export);
        document
            .objects
            .values()
            .filter_map(|object| object.as_dict().ok())
            .filter(|dict| dict.get(b"Type").and_then(|kind| kind.as_name()).ok() == Some(b"FontDescriptor"))
            .map(|dict| {
                let name = String::from_utf8(dict.get(b"FontName").unwrap().as_name().unwrap().to_vec()).unwrap();
                // Without the tag of the subset the PDF embeds, and the style.
                let name = name.split_once('+').map_or(name.as_str(), |(_, name)| name);
                name.split_once('-').map_or(name, |(family, _)| family).to_string()
            })
            .collect()
    }

    fn export_with(core: &Core, reader: &Principal, area_id: &str, pdf_style_id: Option<&str>) -> Result<PdfExport, OpError> {
        let input = ToExportPdf { area_id: area_id.into(), entry_id: None, subtitle: None, pdf_style_id: pdf_style_id.map(Into::into) };
        export_pdf(core, reader, input).map(|reply| reply.output)
    }

    fn style(core: &Core, admin: &Principal, values: PdfStyleValues) -> String {
        create_pdf_style(core, admin, values).unwrap().output.id
    }

    fn standard_values(core: &Core, reader: &Principal) -> PdfStyleValues {
        let list = list_pdf_styles(core, reader, Nothing {}).unwrap().output;
        list.pdf_styles.into_iter().find(|style| style.is_default).unwrap().values
    }

    #[test]
    fn the_default_pdf_style_is_used_unless_another_is_chosen() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        page(&core, &grace, &area_id, None, "Runbook", "Restart the server.");
        let bare = PdfStyleValues { name: "Bare".into(), cover: false, toc: false, ..standard_values(&core, &grace) };
        let bare = style(&core, &ada, bare);

        let with_standard = export_with(&core, &grace, &area_id, None).unwrap();
        set_default_pdf_style(&core, &ada, PdfStyleId { id: bare }).unwrap();
        let with_bare = export_with(&core, &grace, &area_id, None).unwrap();
        let chosen_standard = export_with(&core, &grace, &area_id, Some("standard")).unwrap();
        let made_up = export_with(&core, &grace, &area_id, Some("made-up")).map(|_| ()).unwrap_err();

        assert_eq!(pages_of(&with_standard).len(), 3);
        assert_eq!(pages_of(&with_bare), [squashed("ENGINEERING Runbook Restart the server. 1")]);
        assert_eq!(pages_of(&chosen_standard), pages_of(&with_standard));
        assert_eq!((made_up.kind(), made_up.message()), ("not_found", "There is no such PDF Style."));
    }

    #[test]
    fn the_colours_fonts_and_sizes_of_a_pdf_style_change_the_pdf() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        page(&core, &grace, &area_id, None, "Runbook", "Restart [the server](https://example.org) with `turtle`.\n\n## Install\n\n### Details");
        let standard = export_with(&core, &grace, &area_id, None).unwrap();
        let handout = PdfStyleValues {
            name: "Handout".into(),
            text_color: "#102030".into(),
            heading_color: "#405060".into(),
            accent_color: "#a03010".into(),
            body_font: "Source Serif 4".into(),
            heading_font: "JetBrains Mono".into(),
            code_font: "Inter".into(),
            body_size: 11.0,
            h1_size: 30.0,
            h2_size: 16.0,
            h3_size: 12.0,
            ..standard_values(&core, &grace)
        };
        let handout = style(&core, &ada, handout);

        let styled = export_with(&core, &grace, &area_id, Some(&handout)).unwrap();

        for color in ["#23252b", "#17181c", "#2f5f8a"] {
            assert!(colors_in(&standard).contains(color), "{color} in {:?}", colors_in(&standard));
            assert!(!colors_in(&styled).contains(color), "{color} in {:?}", colors_in(&styled));
        }
        for color in ["#102030", "#405060", "#a03010"] {
            assert!(colors_in(&styled).contains(color), "{color} in {:?}", colors_in(&styled));
        }
        for size in ["9.6", "26", "13.5", "11"] {
            assert!(text_sizes_in(&standard).contains(size), "{size} in {:?}", text_sizes_in(&standard));
        }
        for size in ["11", "30", "16", "12"] {
            assert!(text_sizes_in(&styled).contains(size), "{size} in {:?}", text_sizes_in(&styled));
        }
        for size in ["9.6", "26", "13.5"] {
            assert!(!text_sizes_in(&styled).contains(size), "{size} in {:?}", text_sizes_in(&styled));
        }
        assert_eq!(fonts_in(&standard), BTreeSet::from(["Inter".into(), "JetBrainsMono".into()]));
        assert_eq!(fonts_in(&styled), BTreeSet::from(["Inter".into(), "JetBrainsMono".into(), "SourceSerif4".into()]));
    }

    #[test]
    fn a_pdf_style_leaves_out_the_cover_and_sets_the_depth_of_the_contents_and_the_footer() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        page(&core, &grace, &area_id, None, "Runbook", "## Install\n\n### Details");
        let standard = standard_values(&core, &grace);
        let deep = style(&core, &ada, PdfStyleValues { name: "Deep".into(), cover: false, toc_depth: 3, footer_text: Some("Internal use only".into()), ..standard.clone() });
        let shallow = style(&core, &ada, PdfStyleValues { name: "Shallow".into(), cover: false, toc_depth: 1, ..standard });

        let deep = pages_of(&export_with(&core, &grace, &area_id, Some(&deep)).unwrap());
        let shallow = pages_of(&export_with(&core, &grace, &area_id, Some(&shallow)).unwrap());

        assert_eq!(
            deep,
            [
                "ENGINEERING Contents Runbook 2 Install 2 Details 2 Internal use only 1",
                "ENGINEERING Runbook Install Details Internal use only 2",
            ]
            .map(squashed)
        );
        assert_eq!(shallow[0], squashed("ENGINEERING Contents Runbook 2 1"));
    }

    #[test]
    fn a_pdf_style_in_german_shows_the_pdfs_own_words_and_date_in_german() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        page(&core, &grace, &area_id, None, "Runbook", "> [!TIP]\n> Restart it.");
        let german = PdfStyleValues { name: "Deutsch".into(), language: Language::German, ..standard_values(&core, &grace) };
        let german = style(&core, &ada, german);

        let pages = pages_of(&export_with(&core, &grace, &area_id, Some(&german)).unwrap());

        assert_eq!(
            pages,
            ["Engineering STAND 14. November 2023", "ENGINEERING Inhalt Runbook 3 2", "ENGINEERING Runbook TIPP Restart it. 3"]
                .map(squashed)
        );
    }

    /// The size of every image or drawing the PDF places, page by page, as
    /// (width, height) in whole tenths of a millimetre.
    fn placed_in(export: &PdfExport) -> Vec<Vec<(i64, i64)>> {
        let document = pdf_of(export);
        let mut pages = Vec::new();
        for page_id in document.get_pages().into_values() {
            let content = pdf_extract::content::Content::decode(&document.get_page_content(page_id).unwrap()).unwrap();
            // The current transformation, [a b c d e f], and those saved.
            let mut matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0f32];
            let mut saved = Vec::new();
            let mut placed = Vec::new();
            for operation in content.operations {
                match operation.operator.as_str() {
                    "q" => saved.push(matrix),
                    "Q" => matrix = saved.pop().unwrap(),
                    "cm" => {
                        let n: Vec<f32> = operation.operands.iter().map(|operand| operand.as_float().unwrap()).collect();
                        let m = matrix;
                        matrix = [
                            n[0] * m[0] + n[1] * m[2],
                            n[0] * m[1] + n[1] * m[3],
                            n[2] * m[0] + n[3] * m[2],
                            n[2] * m[1] + n[3] * m[3],
                            n[4] * m[0] + n[5] * m[2] + m[4],
                            n[4] * m[1] + n[5] * m[3] + m[5],
                        ];
                    }
                    "Do" => {
                        let tenths = |points: f32| (points * 254.0 / 72.0).round() as i64;
                        placed.push((tenths(matrix[0].hypot(matrix[1])), tenths(matrix[2].hypot(matrix[3]))));
                    }
                    _ => {}
                }
            }
            pages.push(placed);
        }
        pages
    }

    /// A PNG four pixels wide and one high.
    const WIDE_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAQAAAABCAIAAAB2XpiaAAAADUlEQVR4nGNQcEiAIwASjQMB99WrigAAAABJRU5ErkJggg==";
    /// A PNG one pixel wide and four high.
    const TALL_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAAECAIAAADAusJtAAAADklEQVR4nGNQcEhgQMIAFxADASm6FqEAAAAASUVORK5CYII=";

    fn set_logo(core: &Core, admin: &Principal, pdf_style_id: &str, content: &str) {
        let input = NewPdfStyleLogo { pdf_style_id: pdf_style_id.into(), file_name: "logo".into(), content: content.into() };
        set_pdf_style_logo(core, admin, input).unwrap();
    }

    #[test]
    fn the_logo_is_on_the_cover_and_small_in_every_footer_after_it_in_proportion() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        page(&core, &grace, &area_id, None, "Runbook", "Restart the server.");

        set_logo(&core, &ada, "standard", WIDE_PNG);
        let wide = export_with(&core, &grace, &area_id, None).unwrap();
        set_logo(&core, &ada, "standard", TALL_PNG);
        let tall = export_with(&core, &grace, &area_id, None).unwrap();

        // At most 40 mm wide and 20 mm high on the cover, 6 mm high in the footer.
        assert_eq!(placed_in(&wide), [vec![(400, 100)], vec![(240, 60)], vec![(240, 60)]]);
        assert_eq!(placed_in(&tall), [vec![(50, 200)], vec![(15, 60)], vec![(15, 60)]]);
        assert_eq!(pages_of(&wide), pages_of(&tall));
        assert_eq!(pages_of(&wide)[2], squashed("ENGINEERING Runbook Restart the server. 3"));
    }

    #[test]
    fn an_svg_logo_is_drawn_into_the_pdf() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        page(&core, &grace, &area_id, None, "Runbook", "Restart the server.");
        // 20 by 10, so as wide as the cover allows.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="#aa3300"/></svg>"##;
        set_logo(&core, &ada, "standard", &base64::engine::general_purpose::STANDARD.encode(svg));

        let exported = export_with(&core, &grace, &area_id, None).unwrap();

        assert!(colors_in(&exported).contains("#aa3300"), "{:?}", colors_in(&exported));
        assert_eq!(pages_of(&exported).len(), 3);
    }

    #[test]
    fn a_pdf_style_without_a_logo_gives_the_same_pdf_as_before_it_had_one() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        page(&core, &grace, &area_id, None, "Runbook", "Restart the server.");
        let before = export_with(&core, &grace, &area_id, None).unwrap();

        set_logo(&core, &ada, "standard", WIDE_PNG);
        let with_logo = export_with(&core, &grace, &area_id, None).unwrap();
        remove_pdf_style_logo(&core, &ada, OfPdfStyle { pdf_style_id: "standard".into() }).unwrap();
        let after = export_with(&core, &grace, &area_id, None).unwrap();

        assert_eq!(placed_in(&before), [vec![], vec![], vec![]]);
        assert_ne!(with_logo.content, before.content);
        assert_eq!(after.content, before.content);
    }

    #[test]
    fn a_logo_file_that_cannot_be_shown_any_more_leaves_the_pdf_without_a_logo() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        page(&core, &grace, &area_id, None, "Runbook", "Restart the server.");
        let before = export_with(&core, &grace, &area_id, None).unwrap();
        set_logo(&core, &ada, "standard", WIDE_PNG);
        for file in std::fs::read_dir(core.pdf_logos_dir()).unwrap() {
            std::fs::write(file.unwrap().path(), b"damaged").unwrap();
        }

        let damaged = export_with(&core, &grace, &area_id, None).unwrap();

        assert_eq!(damaged.content, before.content);
    }

    /// The fonts the PDF's text is in, by their full names, such as "Inter-Bold".
    fn font_names_in(export: &PdfExport) -> BTreeSet<String> {
        let document = pdf_of(export);
        document
            .objects
            .values()
            .filter_map(|object| object.as_dict().ok())
            .filter(|dict| dict.get(b"Type").and_then(|kind| kind.as_name()).ok() == Some(b"FontDescriptor"))
            .map(|dict| {
                let name = String::from_utf8(dict.get(b"FontName").unwrap().as_name().unwrap().to_vec()).unwrap();
                // Without the tag of the subset the PDF embeds.
                name.split_once('+').map_or(name.clone(), |(_, name)| name.to_string())
            })
            .collect()
    }

    #[test]
    fn an_uploaded_family_is_used_with_its_bold_and_italic() {
        use crate::ops::fonts::{upload_font, NewFont};
        use crate::ops::testing::{font_file, TestStyle};
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        page(&core, &grace, &area_id, None, "Runbook", "Restart **the server** *gently*, `now`.");
        for (file_name, style) in [("Regular.ttf", TestStyle::Regular), ("Bold.otf", TestStyle::Bold), ("Italic.ttf", TestStyle::Italic)] {
            let content = base64::engine::general_purpose::STANDARD.encode(font_file("Corporate Sans", style));
            upload_font(&core, &ada, NewFont { file_name: file_name.into(), content }).unwrap();
        }
        let corporate = PdfStyleValues {
            name: "Corporate".into(),
            body_font: "Corporate Sans".into(),
            heading_font: "Corporate Sans".into(),
            code_font: "Corporate Sans".into(),
            ..standard_values(&core, &grace)
        };
        let corporate = style(&core, &ada, corporate);

        let styled = export_with(&core, &grace, &area_id, Some(&corporate)).unwrap();

        assert_eq!(
            font_names_in(&styled),
            BTreeSet::from(["CorporateSans-Regular".into(), "CorporateSans-Bold".into(), "CorporateSans-Italic".into()])
        );
        assert_eq!(pages_of(&styled)[2], squashed("ENGINEERING Runbook Restart the server gently, now. 3"));
    }
}
