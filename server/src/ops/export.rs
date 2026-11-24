//! Export: an Area or a Subtree as a ZIP of Markdown files that is readable
//! without Turtle. Directories mirror the tree, each Page is a file named
//! after its title, and frontmatter keeps what a later import needs.

use super::tree::{self, Entry, Kind};
use super::{areas, attachments, links, permissions, tags};
use crate::model::{Block, Inline, Marks};
use super::{Catalogue, Core, OpError, OpResult, Principal};
use anyhow::Context;
use base64::Engine;
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Write;

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "export",
            "Exports an Area, or the Subtree of one of its Pages or Folders, as a ZIP of Markdown files, returned in base64. Directories mirror the tree: every Page and Folder is a Markdown file named after its title, and a directory of the same name holds what is beneath it, a Page's Attachments in `_attachments`. Each file starts with frontmatter holding the ID, whether it is a page or a folder, the title, the position among its siblings, and a Page's Tags. Links to Pages in the export are relative paths; links to other Pages keep their `turtle:` address. Pages the caller cannot read are left out. Needs the Read Role.",
            export,
        )
        .changes = false;
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ToExport {
    pub area_id: String,
    /// The Page or Folder whose Subtree to export; the whole Area if absent.
    #[serde(default)]
    pub entry_id: Option<String>,
}

/// A ZIP of Markdown files.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Export {
    /// A name to save it under, after the Area or the top of the Subtree,
    /// such as "Engineering.zip".
    pub file_name: String,
    /// The ZIP, in base64.
    pub content: String,
}

pub fn export(core: &Core, principal: &Principal, input: ToExport) -> OpResult<Export> {
    areas::signed_in(principal)?;
    // What goes where is decided under one lock, so the tree is seen as it
    // was at one moment; the content is read afterwards, as each Page is now.
    let (name, placed) = {
        let db = core.db();
        let (name, readable) = readable_subtree(&db, principal, &input.area_id, input.entry_id.as_deref())?;
        let mut placed = Vec::new();
        place(&db, &readable, &[], &mut Names::default(), &mut placed)?;
        (name, placed)
    };
    let addresses = Addresses::of(&placed);

    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for entry in &placed {
        zip.start_file(entry.file(), options).context("adding a file")?;
        zip.write_all(entry_file(core, principal, entry, &addresses)?.as_bytes()).context("writing a file")?;
        if entry.has_directory {
            zip.add_directory(entry.directory(), options).context("adding a directory")?;
        }
        if !entry.attachments.is_empty() {
            zip.add_directory(format!("{}{ATTACHMENTS}/", entry.directory()), options).context("adding a directory")?;
        }
        for attachment in &entry.attachments {
            let stored = core.attachments_dir().join(&attachment.content_hash);
            let content = match std::fs::read(&stored) {
                Ok(content) => content,
                // Its Page was removed for good since the tree was read,
                // taking the file along: it is left out, as the Page would
                // have been a moment later.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(anyhow::Error::from(error).context(format!("reading {}", stored.display())).into()),
            };
            zip.start_file(attachment.path.join("/"), options).context("adding a file")?;
            zip.write_all(&content).context("writing a file")?;
        }
    }
    let zip = zip.finish().context("finishing the ZIP")?.into_inner();
    Ok(Export {
        file_name: format!("{}.zip", file_name_of(&name)),
        content: base64::engine::general_purpose::STANDARD.encode(zip),
    }
    .into())
}

/// What an Export of the Area, or of the Subtree of `entry_id` in it, holds:
/// its name, after the Area or the top of the Subtree, and the Pages and
/// Folders the principal can read, in tree order.
pub(super) fn readable_subtree(
    db: &Connection,
    principal: &Principal,
    area_id: &str,
    entry_id: Option<&str>,
) -> Result<(String, Vec<Entry>), OpError> {
    let area = areas::readable_area(db, principal, area_id)?;
    let (name, top) = match entry_id {
        None => (area.name.clone(), tree::beneath(db, &area.id, &tree::root_of(db, &area.id)?)?),
        Some(id) => {
            let found = tree::readable_entry(db, principal, id, None)?;
            if found.area_id != area.id {
                return Err(OpError::NotFound("There is no such Page or Folder.".into()));
            }
            let children = tree::beneath(db, &area.id, id)?;
            let top = Entry { id: id.to_string(), kind: found.kind, title: found.title.clone(), children };
            (found.title, vec![top])
        }
    };
    Ok((name, readable_tree(db, principal, top)?))
}

/// The entries the principal can read. One they cannot read is left out,
/// and what they can read beneath it takes its place.
fn readable_tree(db: &Connection, principal: &Principal, entries: Vec<Entry>) -> Result<Vec<Entry>, OpError> {
    let mut kept = Vec::new();
    for entry in entries {
        let children = readable_tree(db, principal, entry.children)?;
        if permissions::role_on(db, principal, &entry.id)?.is_some() {
            kept.push(Entry { children, ..entry });
        } else {
            kept.extend(children);
        }
    }
    Ok(kept)
}

/// A Page or Folder with its place in the export.
struct Placed {
    id: String,
    kind: Kind,
    title: String,
    /// The directory it is in, from the top of the export down.
    within: Vec<String>,
    /// Its name in that directory: a Page's file adds `.md`.
    name: String,
    /// Counted from 1 among its siblings in the export.
    position: usize,
    tags: Vec<String>,
    /// Whether it has a directory, for what is beneath it: a Folder always does.
    has_directory: bool,
    /// A Page's Attachments, which are in its directory.
    attachments: Vec<PlacedAttachment>,
}

struct PlacedAttachment {
    id: String,
    content_hash: String,
    /// From the top of the export down, its name last.
    path: Vec<String>,
}

impl Placed {
    fn directory(&self) -> String {
        format!("{}/", path(&self.within, &self.name))
    }

    fn file(&self) -> String {
        self.file_path().join("/")
    }

    /// Its file, from the top of the export down.
    fn file_path(&self) -> Vec<String> {
        let mut path = self.within.clone();
        path.push(format!("{}.md", self.name));
        path
    }
}

fn path(within: &[String], name: &str) -> String {
    within.iter().map(String::as_str).chain([name]).collect::<Vec<_>>().join("/")
}

/// Gives each of these siblings, and everything beneath them, its place in
/// the directory `within`, in which `names` are taken.
fn place(db: &Connection, entries: &[Entry], within: &[String], names: &mut Names, out: &mut Vec<Placed>) -> Result<(), OpError> {
    for (index, entry) in entries.iter().enumerate() {
        // Every Entry is a file, and a directory of the same name holds what is beneath it.
        let name = names.claim(&file_name_of(&entry.title), &["", ".md"]);
        let attachments = match entry.kind {
            Kind::Page => attachments::stored_of(db, &entry.id)?,
            Kind::Folder => Vec::new(),
        };
        let mut beneath = within.to_vec();
        beneath.push(name.clone());
        let at = out.len();
        out.push(Placed {
            id: entry.id.clone(),
            kind: entry.kind,
            title: entry.title.clone(),
            within: within.to_vec(),
            name,
            position: index + 1,
            tags: tags::tags_of(db, &entry.id)?,
            has_directory: entry.kind == Kind::Folder || !entry.children.is_empty() || !attachments.is_empty(),
            attachments: Vec::new(),
        });
        let mut names_beneath = Names::default();
        // Attachments have a directory of their own, so an import tells
        // them from Pages; a Page beneath named like it makes way.
        if entry.kind == Kind::Page {
            names_beneath.claim(ATTACHMENTS, &[""]);
        }
        place(db, &entry.children, &beneath, &mut names_beneath, out)?;
        let mut attachment_names = Names::default();
        out[at].attachments = attachments
            .into_iter()
            .map(|(attachment, content_hash)| {
                let whole = file_name_of(&attachment.name);
                let (stem, extension) = match whole.rsplit_once('.') {
                    Some((stem, extension)) if !stem.is_empty() => (stem.to_string(), format!(".{extension}")),
                    _ => (whole, String::new()),
                };
                let name = attachment_names.claim(&stem, &[&extension]);
                let mut path = beneath.clone();
                path.extend([ATTACHMENTS.to_string(), format!("{name}{extension}")]);
                PlacedAttachment { id: attachment.id, content_hash, path }
            })
            .collect();
    }
    Ok(())
}

/// The directory in a Page's directory that holds its Attachments.
const ATTACHMENTS: &str = "_attachments";

/// Where each Page and Attachment of the export is, from its top down, by
/// ID, and each Page's title.
struct Addresses {
    pages: HashMap<String, (Vec<String>, String)>,
    attachments: HashMap<String, Vec<String>>,
}

impl Addresses {
    fn of(placed: &[Placed]) -> Addresses {
        let pages = placed
            .iter()
            .filter(|entry| entry.kind == Kind::Page)
            .map(|page| (page.id.clone(), (page.file_path(), page.title.clone())))
            .collect();
        let attachments = placed
            .iter()
            .flat_map(|page| &page.attachments)
            .map(|attachment| (attachment.id.clone(), attachment.path.clone()))
            .collect();
        Addresses { pages, attachments }
    }

    /// Where an Attachment address of Turtle's points in the export, if it
    /// is to one of the export's Attachments.
    fn attachment(&self, address: &str) -> Option<&Vec<String>> {
        self.attachments.get(attachment_id(address)?)
    }
}

/// How content refers to an Attachment, followed by its ID.
const ATTACHMENT_ADDRESS: &str = "/api/attachments/";

/// The ID of the Attachment an address of Turtle's points to, if it points to one.
pub(super) fn attachment_id(address: &str) -> Option<&str> {
    address.strip_prefix(ATTACHMENT_ADDRESS).filter(|id| !id.is_empty())
}

/// The path from a file in the directory `from` to `to`, as a link writes
/// it. `#` and `%` would be read as part of an address, so they are encoded.
fn relative(from: &[String], to: &[String]) -> String {
    let (directory, _) = to.split_at(to.len() - 1);
    let shared = from.iter().zip(directory).take_while(|(a, b)| a == b).count();
    let up = std::iter::repeat_n("..", from.len() - shared);
    let down = to[shared..].iter().map(String::as_str);
    let path = up.chain(down).collect::<Vec<_>>().join("/");
    path.replace('%', "%25").replace('#', "%23")
}

/// Points what refers to the export's Pages and Attachments at their files,
/// as seen from a file in the directory `from`.
fn point_into_export(blocks: &mut [Block], from: &[String], addresses: &Addresses) {
    for_each_inline(blocks, &mut |inline| match inline {
        Inline::Image { src, .. } => {
            if let Some(to) = addresses.attachment(src) {
                *src = relative(from, to);
            }
        }
        Inline::Text { marks: Marks { link: Some(href), .. }, .. } => {
            if let Some(to) = addresses.attachment(href) {
                *href = relative(from, to);
            }
        }
        // A file has no Page link, so one becomes a link to the file, with
        // the Page's title as its text.
        Inline::PageLink { page_id } => {
            if let Some((to, title)) = addresses.pages.get(page_id) {
                let marks = Marks { link: Some(relative(from, to)), ..Marks::default() };
                *inline = Inline::Text { text: title.clone(), marks };
            }
        }
        Inline::Text { .. } | Inline::HardBreak | Inline::UnknownDirective { .. } => {}
    });
}

fn for_each_inline(blocks: &mut [Block], change: &mut impl FnMut(&mut Inline)) {
    for block in blocks {
        match block {
            Block::Paragraph(content) | Block::Heading { content, .. } => content.iter_mut().for_each(&mut *change),
            Block::Blockquote(children) | Block::Callout { content: children, .. } => for_each_inline(children, change),
            Block::BulletList(items) | Block::OrderedList { items, .. } => {
                items.iter_mut().for_each(|item| for_each_inline(item, change))
            }
            Block::TaskList(items) => items.iter_mut().for_each(|item| for_each_inline(&mut item.content, change)),
            Block::Table(rows) => rows.iter_mut().flatten().flatten().for_each(&mut *change),
            Block::CodeBlock { .. } | Block::HorizontalRule | Block::UnknownDirective { .. } => {}
        }
    }
}

/// The names taken in one directory. File systems that ignore case would
/// take two names differing only in case for one.
#[derive(Default)]
struct Names(HashSet<String>);

impl Names {
    /// The first of `stem`, `stem (2)`, `stem (3)`… that is free with every
    /// suffix, all of which it then takes.
    fn claim(&mut self, stem: &str, suffixes: &[&str]) -> String {
        let candidates = std::iter::once(stem.to_string()).chain((2..).map(|n| format!("{stem} ({n})")));
        for candidate in candidates {
            let taken: Vec<String> = suffixes.iter().map(|suffix| format!("{candidate}{suffix}").to_lowercase()).collect();
            if taken.iter().all(|name| !self.0.contains(name)) {
                self.0.extend(taken);
                return candidate;
            }
        }
        unreachable!("the candidates never run out")
    }
}

/// A name for a file that any common file system accepts and that shows the
/// title: characters some refuse become `-`, and it is never hidden or empty.
pub(super) fn file_name_of(title: &str) -> String {
    let replaced: String = title
        .chars()
        .map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '-' } else { c })
        .collect();
    let trimmed = replaced.trim().trim_start_matches('.').trim_end_matches(['.', ' ']).trim_start();
    // Most file systems allow 255 bytes; this leaves room for ` (2).md`.
    let mut name = String::new();
    for c in trimmed.chars() {
        if name.len() + c.len_utf8() > 200 {
            break;
        }
        name.push(c);
    }
    // Windows refuses these names, whatever follows a dot.
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1",
        "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let base = name.split('.').next().unwrap_or_default().trim_end();
    if name.is_empty() {
        "Untitled".into()
    } else if RESERVED.iter().any(|reserved| reserved.eq_ignore_ascii_case(base)) {
        format!("{name}-")
    } else {
        name
    }
}

/// An Entry's file: frontmatter, then a Page's content in Markdown.
fn entry_file(core: &Core, principal: &Principal, entry: &Placed, addresses: &Addresses) -> Result<String, OpError> {
    let quoted = |text: &str| serde_json::to_string(text).expect("a string serialises");
    let mut file = format!(
        "---\nid: {}\nkind: {}\ntitle: {}\nposition: {}\n",
        quoted(&entry.id),
        entry.kind.stored(),
        quoted(&entry.title),
        entry.position,
    );
    if entry.kind == Kind::Folder {
        file.push_str("---\n");
        return Ok(file);
    }
    let page = entry;
    let tags: Vec<String> = page.tags.iter().map(|tag| quoted(tag)).collect();
    file.push_str(&format!("tags: [{}]\n---\n", tags.join(", ")));
    let mut blocks = core.live().content(&page.id)?;
    point_into_export(&mut blocks, &page.within, addresses);
    // A link to a Page outside the export stays a Page link, shown as
    // get_page_markdown shows it.
    let markdown = links::markdown_for(core, principal, &blocks)?;
    if !markdown.is_empty() {
        file.push('\n');
        file.push_str(&markdown);
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::attachments::{attach_file, NewAttachment};
    use crate::ops::content::{replace_page, NewContent};
    use crate::ops::permissions::{Holder, Role};
    use crate::ops::testing::{fresh, give_role, member};
    use crate::ops::tags::{add_tag, PageTag};
    use crate::ops::tree::{create_folder, create_page, NewEntry, NewPage};
    use base64::Engine;
    use std::collections::BTreeMap;
    use std::io::Read;

    fn area(core: &Core, admin: &Principal, name: &str) -> String {
        create_area(core, admin, NewArea { name: name.into() }).unwrap().output.id
    }

    /// Creates a Page with content, beneath `parent` or at the top of the Area.
    fn page(core: &Core, writer: &Principal, area_id: &str, parent: Option<&str>, title: &str, markdown: &str) -> String {
        let entry = NewEntry { area_id: area_id.into(), parent_id: parent.map(Into::into), title: title.into() };
        let markdown = (!markdown.is_empty()).then(|| markdown.to_string());
        create_page(core, writer, NewPage { entry, markdown }).unwrap().output.id
    }

    /// What the ZIP holds: each file's path with its text, and each
    /// directory's path, ending in `/`, with nothing.
    fn unzip(export: &Export) -> BTreeMap<String, String> {
        let bytes = base64::engine::general_purpose::STANDARD.decode(&export.content).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut files = BTreeMap::new();
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).unwrap();
            let mut text = String::new();
            file.read_to_string(&mut text).unwrap();
            files.insert(file.name().unwrap().to_string(), text);
        }
        files
    }

    fn export_area(core: &Core, reader: &Principal, area_id: &str) -> BTreeMap<String, String> {
        unzip(&export(core, reader, ToExport { area_id: area_id.into(), entry_id: None }).unwrap().output)
    }

    #[test]
    fn an_area_is_exported_as_markdown_files_in_directories_that_mirror_its_tree() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let runbook = page(&core, &grace, &area_id, None, "Runbook", "Restart the *server*.");
        let deploy = page(&core, &grace, &area_id, Some(&runbook), "Deploy", "");

        let files = export_area(&core, &grace, &area_id);

        assert_eq!(files.keys().collect::<Vec<_>>(), ["Runbook.md", "Runbook/", "Runbook/Deploy.md"]);
        assert_eq!(
            files["Runbook.md"],
            format!("---\nid: \"{runbook}\"\nkind: page\ntitle: \"Runbook\"\nposition: 1\ntags: []\n---\n\nRestart the *server*.\n")
        );
        assert_eq!(
            files["Runbook/Deploy.md"],
            format!("---\nid: \"{deploy}\"\nkind: page\ntitle: \"Deploy\"\nposition: 1\ntags: []\n---\n")
        );
    }

    #[test]
    fn directives_turtle_does_not_know_are_exported_as_they_were_written() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let content = "Plan :acme.demo[hi *there*]{x=1} today.\n\n:::acme.board{x=1}\n- [ ] *odd*   spacing\n:::\n";
        page(&core, &grace, &area_id, None, "Board", content);

        let files = export_area(&core, &grace, &area_id);

        assert!(files["Board.md"].ends_with(&format!("---\n\n{content}")), "{}", files["Board.md"]);
    }

    /// The frontmatter line of a file that starts with the given key.
    fn frontmatter<'a>(file: &'a str, key: &str) -> &'a str {
        file.lines().find(|line| line.starts_with(&format!("{key}: "))).unwrap()
    }

    #[test]
    fn file_names_come_from_titles_and_are_unique_among_siblings() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let folder = NewEntry { area_id: area_id.clone(), parent_id: None, title: "Notes".into() };
        let notes = create_folder(&core, &grace, folder).unwrap().output.id;
        page(&core, &grace, &area_id, None, "notes", "");
        page(&core, &grace, &area_id, None, "Notes", "");
        page(&core, &grace, &area_id, None, "CI/CD: \"why?\"", "");
        page(&core, &grace, &area_id, None, ".env", "");
        page(&core, &grace, &area_id, None, "con.txt", "");

        let files = export_area(&core, &grace, &area_id);

        assert_eq!(
            files.keys().collect::<Vec<_>>(),
            ["CI-CD- -why--.md", "Notes (3).md", "Notes.md", "Notes/", "con.txt-.md", "env.md", "notes (2).md"]
        );
        // A Folder's file holds only what an import needs to make it again.
        assert_eq!(files["Notes.md"], format!("---\nid: \"{notes}\"\nkind: folder\ntitle: \"Notes\"\nposition: 1\n---\n"));
        assert_eq!(frontmatter(&files["notes (2).md"], "position"), "position: 2");
        assert_eq!(frontmatter(&files["Notes (3).md"], "title"), "title: \"Notes\"");
        assert_eq!(frontmatter(&files["Notes (3).md"], "position"), "position: 3");
        assert_eq!(frontmatter(&files["CI-CD- -why--.md"], "title"), "title: \"CI/CD: \\\"why?\\\"\"");
    }

    #[test]
    fn each_file_carries_its_pages_tags() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let runbook = page(&core, &grace, &area_id, None, "Runbook", "");
        for tag in ["Ops", "Postmortem"] {
            add_tag(&core, &grace, PageTag { page_id: runbook.clone(), tag: tag.into() }).unwrap();
        }

        let files = export_area(&core, &grace, &area_id);

        assert_eq!(frontmatter(&files["Runbook.md"], "tags"), "tags: [\"ops\", \"postmortem\"]");
    }

    fn write(core: &Core, writer: &Principal, page_id: &str, markdown: &str) {
        replace_page(core, writer, NewContent { page_id: page_id.into(), markdown: markdown.into() }).unwrap();
    }

    fn attach(core: &Core, writer: &Principal, page_id: &str, name: &str, content: &str) -> String {
        let content = base64::engine::general_purpose::STANDARD.encode(content);
        let input = NewAttachment { page_id: page_id.into(), name: name.into(), content };
        attach_file(core, writer, input).unwrap().output.id
    }

    #[test]
    fn attachments_sit_apart_in_their_pages_directory_and_are_referenced_by_relative_path() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let runbook = page(&core, &grace, &area_id, None, "Runbook", "");
        page(&core, &grace, &area_id, Some(&runbook), "Deploy", "");
        page(&core, &grace, &area_id, Some(&runbook), "_attachments", "");
        let diagram = attach(&core, &grace, &runbook, "diagram.png", "first diagram");
        let second = attach(&core, &grace, &runbook, "Diagram.png", "second diagram");
        let log = attach(&core, &grace, &runbook, "Deploy.md", "a log");
        let markdown = format!(
            "![Flow](/api/attachments/{diagram}) ![Flow](/api/attachments/{second})\n\nSee [the log](/api/attachments/{log})."
        );
        replace_page(&core, &grace, NewContent { page_id: runbook.clone(), markdown }).unwrap();

        let files = export_area(&core, &grace, &area_id);

        assert_eq!(
            files.keys().collect::<Vec<_>>(),
            [
                "Runbook.md",
                "Runbook/",
                "Runbook/Deploy.md",
                "Runbook/_attachments (2).md",
                "Runbook/_attachments/",
                "Runbook/_attachments/Deploy.md",
                "Runbook/_attachments/Diagram (2).png",
                "Runbook/_attachments/diagram.png",
            ]
        );
        assert_eq!(files["Runbook/_attachments/diagram.png"], "first diagram");
        assert_eq!(files["Runbook/_attachments/Diagram (2).png"], "second diagram");
        assert_eq!(files["Runbook/_attachments/Deploy.md"], "a log");
        assert!(files["Runbook.md"].ends_with(
            "\n![Flow](<Runbook/_attachments/diagram.png>) ![Flow](<Runbook/_attachments/Diagram (2).png>)\n\nSee [the log](<Runbook/_attachments/Deploy.md>).\n"
        ));
    }

    #[test]
    fn links_to_pages_in_the_export_are_relative_paths() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let guides = NewEntry { area_id: area_id.clone(), parent_id: None, title: "Guides #1".into() };
        let guides = create_folder(&core, &grace, guides).unwrap().output.id;
        let setup = page(&core, &grace, &area_id, Some(&guides), "Set up", "");
        let runbook = page(&core, &grace, &area_id, None, "Runbook", &format!("Start with [](turtle:{setup})."));
        write(&core, &grace, &setup, &format!("Then read [](turtle:{runbook})."));

        let files = export_area(&core, &grace, &area_id);

        assert!(files["Runbook.md"].ends_with("\nStart with [Set up](<Guides %231/Set up.md>).\n"));
        assert!(files["Guides #1/Set up.md"].ends_with("\nThen read [Runbook](<../Runbook.md>).\n"));
    }

    #[test]
    fn a_subtree_is_exported_alone_and_links_out_of_it_keep_their_turtle_address() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area_id = area(&core, &grace, "Engineering");
        let kens_area = area(&core, &ken, "Payroll");
        let salaries = page(&core, &ken, &kens_area, None, "Salaries", "");
        let overview = page(&core, &grace, &area_id, None, "Overview", "");
        let runbook = page(&core, &grace, &area_id, None, "Runbook", "");
        let deploy = page(&core, &grace, &area_id, Some(&runbook), "Deploy", "");
        let markdown = format!("See [](turtle:{overview}), [](turtle:{salaries}) and [](turtle:{deploy}).");
        write(&core, &grace, &runbook, &markdown);

        let exported = export(&core, &grace, ToExport { area_id: area_id.clone(), entry_id: Some(runbook) }).unwrap().output;
        let files = unzip(&exported);

        assert_eq!(exported.file_name, "Runbook.zip");
        assert_eq!(files.keys().collect::<Vec<_>>(), ["Runbook.md", "Runbook/", "Runbook/Deploy.md"]);
        assert!(files["Runbook.md"].ends_with(&format!(
            "\nSee [Overview](turtle:{overview}), [](turtle:{salaries}) and [Deploy](<Runbook/Deploy.md>).\n"
        )));
    }


    #[test]
    fn pages_the_exporter_cannot_read_are_left_out_and_what_they_can_read_beneath_takes_their_place() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area_id = area(&core, &grace, "Engineering");
        give_role(&core, &area_id, &ken, Role::Read);
        let runbook = page(&core, &grace, &area_id, None, "Runbook", "");
        let secrets = page(&core, &grace, &area_id, Some(&runbook), "Secrets", "");
        let rotation = page(&core, &grace, &area_id, Some(&secrets), "Rotation", "");
        write(&core, &grace, &runbook, &format!("Keep [](turtle:{secrets}) safe."));
        // Secrets has Grants of its own, which leave Ken out; Rotation, beneath it, lets him read again.
        let Principal::Member { id: grace_id, .. } = &grace else { unreachable!() };
        let Principal::Member { id: ken_id, .. } = &ken else { unreachable!() };
        let db = core.db();
        permissions::grant(&db, &secrets, &Holder::Member(grace_id.clone()), Role::Admin).unwrap();
        permissions::grant(&db, &rotation, &Holder::Member(ken_id.clone()), Role::Read).unwrap();
        drop(db);

        let files = export_area(&core, &ken, &area_id);

        assert_eq!(files.keys().collect::<Vec<_>>(), ["Runbook.md", "Runbook/", "Runbook/Rotation.md"]);
        assert!(files["Runbook.md"].ends_with(&format!("\nKeep [](turtle:{secrets}) safe.\n")));
        let hidden = export(&core, &ken, ToExport { area_id: area_id.clone(), entry_id: Some(secrets) }).unwrap_err();
        let made_up = export(&core, &ken, ToExport { area_id, entry_id: Some("made-up".into()) }).unwrap_err();
        assert_eq!((hidden.kind(), hidden.message()), (made_up.kind(), made_up.message()));
        assert_eq!(hidden.kind(), "not_found");
    }


    #[test]
    fn a_page_nobody_has_open_is_exported_as_it_was_saved() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let runbook = page(&core, &grace, &area_id, None, "Runbook", "Restart the server.");
        core.save_changed_pages().unwrap();
        core.live().forget(&runbook);

        let files = export_area(&core, &grace, &area_id);

        assert!(files["Runbook.md"].ends_with("---\n\nRestart the server.\n"));
    }

}
