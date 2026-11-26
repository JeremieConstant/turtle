//! Import: Pages, Folders and Attachments brought into an Area from outside.
//!
//! Every importer has two halves. A reader turns its source into a Bundle,
//! which looks the same whatever the source: `markdown` reads a ZIP of
//! Markdown files, as Turtle exports them (ADR-0011) or from anywhere else.
//! `write_bundle` then creates what a Bundle holds, the same for every
//! source: the tree in order, IDs kept where they are free, Attachments,
//! links pointed at what they name, Tags, and a report of what did not come
//! across instead of a failure. An importer for another source, such as
//! Confluence, is a new reader beside `markdown` and its own operations.
//!
//! Each import is an operation that answers once all is done, and one that
//! starts it and answers at once (`running`), so a large import can be
//! followed while it runs instead of only waited for.

use super::tree::{self, Entry, Kind, NewEntry};
use super::{Catalogue, Core, OpError, OpResult, Principal};
use crate::model::{Block, Inline, Marks};
use std::collections::HashMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod confluence;
mod markdown;
pub(super) mod record;
mod running;

pub use running::Imports;

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue.add(
        "import",
        "Imports a ZIP of Markdown files, sent in base64, into an Area or beneath one of its Pages or Folders: an Export of Turtle's, or any folder of Markdown. Directories become the tree: a Markdown file is a Page, a directory of the same name holds what is beneath it, and a directory without one becomes a Folder. Frontmatter, where a file has it, gives the title, whether it is a page or a folder, the position among its siblings, the Tags and an ID, which is kept if nothing has it yet; without frontmatter a Page is titled after its file name. Relative links between the files become Page links, and files they refer to, and those in a Page's `_attachments` directory, become its Attachments. A file that cannot be imported is reported with the reason and the rest comes in. Needs the Write Role.",
        import,
    );
    catalogue.add(
        "start_import",
        "Starts what import does and answers at once, before anything is read, with the import's ID; get_import_progress then tells how far it has got and, once it is done, what import answers with. Meant for large ZIPs, which take a while to bring in. Needs the Write Role.",
        start_import,
    );
    catalogue
        .add(
            "get_import_progress",
            "Tells how far an import begun with start_import or start_import_confluence has got: reading the ZIP, creating Pages and Folders, writing their content, done or failed, with how many of how many. A Confluence import tells its report as soon as the space is read, before anything is created. Once done, it holds what the import created and what did not come across; once failed, why. Only whoever started an import can follow it, and it is forgotten an hour after it ends.",
            get_import_progress,
        )
        .changes = false;
    confluence::register(catalogue);
    record::register(catalogue);
}

/// The largest ZIP that can be imported at once, unless whoever runs Turtle
/// sets another. An import is held in memory several times over while it is
/// read, so this is far below what a whole Confluence space may need.
pub const DEFAULT_IMPORT_LIMIT: u64 = 200 * 1024 * 1024;

/// Why a ZIP larger than the import limit is refused.
pub fn too_large(core: &Core) -> OpError {
    OpError::InvalidInput(format!(
        "This ZIP is larger than {}, the most this Turtle imports at once. Import it in parts, or ask whoever runs Turtle to raise the limit with TURTLE_MAX_IMPORT_MB.",
        super::attachments::size_in_words(core.import_limit())
    ))
}

/// Why a file in an import does not come across when it is larger than
/// the attachment limit, or nothing when it is not.
pub(super) fn too_large_to_attach(core: &Core, bytes: u64) -> Option<String> {
    let limit = core.attachment_limit();
    (bytes > limit).then(|| {
        format!(
            "It is {}, larger than {}, the most this Turtle accepts for one file, so it was left out. Whoever runs Turtle can raise that limit with TURTLE_MAX_ATTACHMENT_MB.",
            size_rounded_up(bytes),
            super::attachments::size_in_words(limit)
        )
    })
}

/// "31.5 MB": a size as people read it, rounded up, so a file just over a
/// limit never reads as the limit itself.
fn size_rounded_up(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    let tenths = |unit: u64| (bytes * 10).div_ceil(unit);
    let shown = |tenths: u64, unit: &str| {
        if tenths.is_multiple_of(10) { format!("{} {unit}", tenths / 10) } else { format!("{}.{} {unit}", tenths / 10, tenths % 10) }
    };
    if bytes >= MB {
        shown(tenths(MB), "MB")
    } else if bytes >= KB {
        shown(tenths(KB), "KB")
    } else {
        format!("{bytes} bytes")
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ToImport {
    pub area_id: String,
    /// The Page or Folder to import beneath; the top of the Area if absent.
    #[serde(default)]
    pub parent_id: Option<String>,
    /// The ZIP, in base64.
    pub content: String,
}

/// What an import created, and what did not come across.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Imported {
    /// The import as list_imports names it, to undo it with undo_import.
    pub import_id: String,
    /// The Pages and Folders created at the top of the import, with what is
    /// beneath them.
    pub entries: Vec<Entry>,
    /// Each file that did not come across, with the reason. Empty when all did.
    pub problems: Vec<Problem>,
    /// The Page holding the import's report, for an import from Confluence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_page_id: Option<String>,
}

/// What writing a Bundle created: what the import answers with, and the ID
/// each imported Page got, by its key.
pub(super) struct Written {
    pub imported: Imported,
    pub pages: HashMap<String, String>,
}

/// A file that did not come across, and why.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Problem {
    /// Where it is in what was imported, such as its path in the ZIP.
    pub file: String,
    /// Why it did not come across, as a sentence.
    pub message: String,
}

impl Problem {
    pub(super) fn new(file: impl Into<String>, message: impl Into<String>) -> Problem {
        Problem { file: file.into(), message: message.into() }
    }
}

pub fn import(core: &Core, principal: &Principal, input: ToImport) -> OpResult<Imported> {
    // Access is checked before the ZIP is read, so a refused import reads nothing.
    check_target(core, principal, &input.area_id, input.parent_id.as_deref())?;
    let bundle = markdown::read(input.content)?;
    let import_id = record::new_id()?;
    Ok(write_bundle(core, principal, &input.area_id, input.parent_id.as_deref(), bundle, &import_id, &|_| {})?.imported.into())
}

pub fn start_import(core: &Core, principal: &Principal, input: ToImport) -> OpResult<running::ImportProgress> {
    // A refused import is refused at once, not once it is running.
    check_target(core, principal, &input.area_id, input.parent_id.as_deref())?;
    running::start(core, principal, move |core, principal, following| {
        let bundle = markdown::read(input.content)?;
        let step = |step| following.step(step);
        Ok(write_bundle(core, principal, &input.area_id, input.parent_id.as_deref(), bundle, following.import_id(), &step)?.imported)
    })
}

pub fn get_import_progress(core: &Core, principal: &Principal, input: running::OfImport) -> OpResult<running::ImportProgress> {
    running::progress(core, principal, &input.import_id)
}

/// What an importer read from its source, in a form that is the same
/// whatever the source: the Pages and Folders to create, in tree order.
pub(super) struct Bundle {
    /// What is imported, as a phrase: "a ZIP of Markdown files".
    pub source: String,
    pub entries: Vec<Incoming>,
    /// What the importer could not read, to report alongside what the
    /// writing could not bring across.
    pub problems: Vec<Problem>,
}

/// A Page or Folder to create, with what is beneath it.
pub(super) struct Incoming {
    /// What the source calls it, unique in the bundle, such as its path in a
    /// ZIP: links name what they point to by it, and problems name it.
    pub key: String,
    pub kind: Kind,
    pub title: String,
    /// The ID to give it if nothing has it yet, as an Export records it.
    pub id: Option<String>,
    pub content: Vec<Block>,
    /// A Page's Tags, as written; a Folder has none.
    pub tags: Vec<String>,
    /// What the links in the content point to, by the address they are
    /// written with. A link to anything else stays as it is.
    pub links: HashMap<String, Target>,
    /// A Page's Attachments.
    pub attachments: Vec<IncomingAttachment>,
    pub children: Vec<Incoming>,
}

/// A file to attach to a Page.
pub(super) struct IncomingAttachment {
    /// What the source calls it, unique in the bundle, as for an Incoming.
    pub key: String,
    pub name: String,
    pub content: Vec<u8>,
}

/// What a link in imported content points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Target {
    /// The Page with this key, which becomes a Page link.
    Entry(String),
    /// The Attachment with this key, which becomes its address.
    Attachment(String),
}

/// How far writing a bundle has got, as it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    /// `done` of `total` Pages and Folders are created.
    Creating { done: u32, total: u32 },
    /// The content of `done` of `total` Pages is written.
    Writing { done: u32, total: u32 },
}

/// Is told each step of writing a bundle.
pub(super) type Progress<'a> = &'a (dyn Fn(Step) + 'a);

/// Creates what the bundle holds beneath `parent_id`, or at the top of the
/// Area, telling `progress` how far it has got.
pub(super) fn write_bundle(
    core: &Core,
    principal: &Principal,
    area_id: &str,
    parent_id: Option<&str>,
    bundle: Bundle,
    import_id: &str,
    progress: Progress,
) -> Result<Written, OpError> {
    // Checked even with nothing to create, so an empty import is refused like any other.
    check_target(core, principal, area_id, parent_id)?;
    record::begin(core, principal, import_id, area_id, &bundle.source)?;
    // Content is written once everything is created, so a link can point
    // to a Page that comes later in the tree.
    let mut created = Created { import_id: import_id.to_string(), problems: bundle.problems, ..Created::default() };
    let to_create = count(&bundle.entries);
    progress(Step::Creating { done: 0, total: to_create });
    let mut creating = Creating { progress, done: 0, total: to_create };
    let entries = create_all(core, principal, area_id, parent_id, bundle.entries, &mut created, &mut creating)?;
    let contents = std::mem::take(&mut created.content);
    let to_write = contents.len() as u32;
    progress(Step::Writing { done: 0, total: to_write });
    for (written, (page_id, mut content, links)) in contents.into_iter().enumerate() {
        point_links(&mut content, &links, &created);
        super::content::write_content(core, principal, &page_id, content)?;
        progress(Step::Writing { done: written as u32 + 1, total: to_write });
    }
    let imported = Imported { import_id: import_id.to_string(), entries, problems: created.problems, report_page_id: None };
    Ok(Written { imported, pages: created.pages })
}

/// How many Pages and Folders there are, with everything beneath them.
fn count(entries: &[Incoming]) -> u32 {
    entries.iter().map(|entry| 1 + count(&entry.children)).sum()
}

/// How many Pages and Folders are created so far, and whom to tell.
struct Creating<'a> {
    progress: Progress<'a>,
    done: u32,
    total: u32,
}

/// Fails unless the principal may add Pages and Folders where the import goes.
fn check_target(core: &Core, principal: &Principal, area_id: &str, parent_id: Option<&str>) -> Result<(), OpError> {
    super::areas::signed_in(principal)?;
    tree::writable_parent(&core.db(), principal, area_id, parent_id)?;
    Ok(())
}

/// What has been created so far, and what is left to write once all is.
#[derive(Default)]
struct Created {
    /// The import, to record what it creates.
    import_id: String,
    /// The ID each imported Page got, by its key.
    pages: HashMap<String, String>,
    /// The address each imported Attachment got, by its key.
    attachments: HashMap<String, String>,
    /// What did not come across.
    problems: Vec<Problem>,
    /// Each Page's content with the links in it, to write at the end.
    content: Vec<(String, Vec<Block>, HashMap<String, Target>)>,
}

/// Points the links and images in the content that refer to what was
/// imported at it: a Page as a Page link, an Attachment by its address.
/// What refers to something that did not come across, such as a file too
/// large to attach or a Folder, keeps its text without the address, and an
/// image says which file is missing, so nothing points nowhere.
fn point_links(content: &mut [Block], links: &HashMap<String, Target>, created: &Created) {
    super::export::for_each_inline(content, &mut |inline| match inline {
        Inline::Image { src, .. } => {
            if let Some(Target::Attachment(key)) = links.get(src.as_str()) {
                match created.attachments.get(key) {
                    Some(url) => *src = url.clone(),
                    None => *inline = not_imported(key),
                }
            }
        }
        Inline::Text { marks, .. } => {
            let Some(target) = marks.link.as_deref().and_then(|href| links.get(href)) else { return };
            match target {
                Target::Entry(key) => match created.pages.get(key) {
                    Some(page_id) => *inline = Inline::PageLink { page_id: page_id.clone() },
                    None => marks.link = None,
                },
                Target::Attachment(key) => marks.link = created.attachments.get(key).cloned(),
            }
        }
        _ => {}
    });
}

/// What stands where an image was whose file did not come across.
fn not_imported(key: &str) -> Inline {
    let name = key.rsplit('/').next().unwrap_or(key);
    Inline::Text { text: format!("Not imported: {name}"), marks: Marks { italic: true, ..Marks::default() } }
}

fn create_all(
    core: &Core,
    principal: &Principal,
    area_id: &str,
    parent_id: Option<&str>,
    incoming: Vec<Incoming>,
    created_so_far: &mut Created,
    creating: &mut Creating,
) -> Result<Vec<Entry>, OpError> {
    let mut entries = Vec::new();
    for incoming in incoming {
        let entry = NewEntry { area_id: area_id.into(), parent_id: parent_id.map(Into::into), title: incoming.title };
        let mut created = tree::create_with_id(core, principal, entry, incoming.kind, incoming.id.as_deref())?.output;
        record::created(core, &created_so_far.import_id, &created.id)?;
        // A Folder holds neither Attachments nor Tags, which a source such
        // as Confluence may give one: they are told, and the rest comes in.
        let (attachments, tags) = if incoming.kind == Kind::Folder {
            for attachment in incoming.attachments {
                created_so_far.problems.push(Problem::new(attachment.key, "It belongs to a Folder, which holds no Attachments, so it was left out."));
            }
            for tag in incoming.tags {
                created_so_far.problems.push(Problem::new(incoming.key.clone(), format!("Its Tag \"{tag}\" was left out: a Folder has no Tags.")));
            }
            (Vec::new(), Vec::new())
        } else {
            (incoming.attachments, incoming.tags)
        };
        for attachment in attachments {
            if let Some(message) = too_large_to_attach(core, attachment.content.len() as u64) {
                created_so_far.problems.push(Problem::new(attachment.key, message));
                continue;
            }
            match super::attachments::attach(core, principal, created.id.clone(), &attachment.name, attachment.content) {
                Ok(attached) => {
                    created_so_far.attachments.insert(attachment.key, attached.output.url);
                }
                // What is wrong with this one file is told; what is wrong
                // with the import as a whole, such as a missing Role, ends it.
                Err(OpError::InvalidInput(message)) => created_so_far.problems.push(Problem::new(attachment.key, message)),
                Err(error) => return Err(error),
            }
        }
        if incoming.kind == Kind::Page {
            created_so_far.pages.insert(incoming.key.clone(), created.id.clone());
            if !incoming.content.is_empty() {
                created_so_far.content.push((created.id.clone(), incoming.content, incoming.links));
            }
        }
        for tag in tags {
            match super::tags::add_tag(core, principal, super::tags::PageTag { page_id: created.id.clone(), tag: tag.clone() }) {
                Ok(_) => {}
                // A Tag that is not one, such as a Confluence label too long
                // for a Tag, is told, and the Page keeps the rest.
                Err(OpError::InvalidInput(message)) => created_so_far
                    .problems
                    .push(Problem::new(incoming.key.clone(), format!("Its Tag \"{tag}\" was left out: {message}"))),
                Err(error) => return Err(error),
            }
        }
        creating.done += 1;
        (creating.progress)(Step::Creating { done: creating.done, total: creating.total });
        created.children = create_all(core, principal, area_id, Some(&created.id), incoming.children, created_so_far, creating)?;
        entries.push(created);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::attachments::{list_attachments, read_attachment, AttachmentId};
    use crate::ops::content::{get_page_markdown, OfPage};
    use crate::ops::testing::{fresh, member};
    use crate::ops::tree::{get_page, get_tree, Kind, PageId, TreeOfArea};
    use base64::Engine;
    use std::io::Write;

    fn area(core: &Core, admin: &Principal, name: &str) -> String {
        create_area(core, admin, NewArea { name: name.into() }).unwrap().output.id
    }

    /// A ZIP holding these files, by path, in base64.
    fn zip_of(files: &[(&str, &str)]) -> String {
        let files: Vec<(&str, &[u8])> = files.iter().map(|(path, content)| (*path, content.as_bytes())).collect();
        zip_of_bytes(&files)
    }

    fn zip_of_bytes(files: &[(&str, &[u8])]) -> String {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (path, content) in files {
            zip.start_file(*path, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(content).unwrap();
        }
        base64::engine::general_purpose::STANDARD.encode(zip.finish().unwrap().into_inner())
    }

    fn import_into(core: &Core, writer: &Principal, area_id: &str, files: &[(&str, &str)]) -> Imported {
        import(core, writer, ToImport { area_id: area_id.into(), parent_id: None, content: zip_of(files) }).unwrap().output
    }

    /// The tree of an Area as (title, kind) pairs, indented by depth.
    fn outline(core: &Core, reader: &Principal, area_id: &str) -> Vec<String> {
        fn walk(entries: &[Entry], depth: usize, out: &mut Vec<String>) {
            for entry in entries {
                let kind = if entry.kind == Kind::Folder { " (folder)" } else { "" };
                out.push(format!("{}{}{kind}", "  ".repeat(depth), entry.title));
                walk(&entry.children, depth + 1, out);
            }
        }
        let mut out = Vec::new();
        walk(&get_tree(core, reader, TreeOfArea { area_id: area_id.into() }).unwrap().output.entries, 0, &mut out);
        out
    }

    fn markdown_of(core: &Core, reader: &Principal, page_id: &str) -> String {
        get_page_markdown(core, reader, OfPage { page_id: page_id.into() }).unwrap().output.markdown
    }

    #[test]
    fn plain_markdown_files_become_pages_titled_after_their_file_names() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");

        let imported = import_into(&core, &grace, &area_id, &[("Runbook.md", "Restart the *server*.\n")]);

        assert_eq!(outline(&core, &grace, &area_id), ["Runbook"]);
        assert_eq!(markdown_of(&core, &grace, &imported.entries[0].id), "Restart the *server*.\n");
    }

    #[test]
    fn directories_become_the_tree_and_a_directory_without_a_file_of_its_own_a_folder() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");

        import_into(
            &core,
            &grace,
            &area_id,
            &[("Runbook.md", ""), ("Runbook/Deploy.md", ""), ("Guides/Set up.md", ""), ("Guides/Tools/Editor.md", "")],
        );

        assert_eq!(
            outline(&core, &grace, &area_id),
            ["Guides (folder)", "  Set up", "  Tools (folder)", "    Editor", "Runbook", "  Deploy"]
        );
    }

    #[test]
    fn the_title_and_kind_come_from_the_frontmatter_and_the_frontmatter_is_not_content() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");

        let imported = import_into(
            &core,
            &grace,
            &area_id,
            &[
                ("CI-CD- -why--.md", "---\nid: \"x\"\nkind: page\ntitle: \"CI/CD: \\\"why?\\\"\"\nposition: 1\ntags: []\n---\n\nBuild it.\n"),
                ("Notes.md", "---\nkind: folder\ntitle: \"Notes\"\n---\n"),
            ],
        );

        assert_eq!(outline(&core, &grace, &area_id), ["CI/CD: \"why?\"", "Notes (folder)"]);
        assert_eq!(markdown_of(&core, &grace, &imported.entries[0].id), "Build it.\n");
    }

    #[test]
    fn order_among_siblings_follows_the_frontmatter_and_files_without_a_position_come_after_by_name() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");

        import_into(
            &core,
            &grace,
            &area_id,
            &[
                ("Apple.md", "---\nposition: 2\n---\n"),
                ("Mango.md", ""),
                ("Banana.md", ""),
                ("Zebra.md", "---\nposition: 1\n---\n"),
            ],
        );

        assert_eq!(outline(&core, &grace, &area_id), ["Zebra", "Apple", "Banana", "Mango"]);
    }

    #[test]
    fn tags_in_the_frontmatter_are_set_on_the_page() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");

        let imported = import_into(
            &core,
            &grace,
            &area_id,
            &[
                ("Exported.md", "---\ntags: [\"ops\", \"postmortem\"]\n---\n"),
                ("Elsewhere.md", "---\ntags:\n  - Release Notes\n  - '#Ops'\n---\n"),
            ],
        );

        let tags = |index: usize| get_page(&core, &grace, PageId { id: imported.entries[index].id.clone() }).unwrap().output.tags;
        assert_eq!(tags(0), ["ops", "release-notes"]);
        assert_eq!(tags(1), ["ops", "postmortem"]);
    }

    #[test]
    fn relative_links_between_imported_files_become_page_links() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");

        let imported = import_into(
            &core,
            &grace,
            &area_id,
            &[
                ("Guides #1/Set up.md", "Then read [Runbook](<../Runbook.md>)."),
                ("Runbook.md", "Start with [Set up](<Guides %231/Set up.md#first-steps>), not [this](Missing.md)."),
            ],
        );

        let set_up = &imported.entries[0].children[0].id;
        let runbook = &imported.entries[1].id;
        assert_eq!(
            markdown_of(&core, &grace, runbook),
            format!("Start with [Set up](turtle:{set_up}), not [this](<Missing.md>).\n")
        );
        assert_eq!(markdown_of(&core, &grace, set_up), format!("Then read [Runbook](turtle:{runbook}).\n"));
    }

    /// Each Attachment of a Page as its name and content.
    fn attachments_of(core: &Core, reader: &Principal, page_id: &str) -> Vec<(String, String, String)> {
        let listed = list_attachments(core, reader, PageId { id: page_id.into() }).unwrap().output.attachments;
        listed
            .into_iter()
            .map(|attachment| {
                let read = read_attachment(core, reader, AttachmentId { id: attachment.id }).unwrap().output;
                let content = base64::engine::general_purpose::STANDARD.decode(read.content).unwrap();
                (attachment.name, String::from_utf8(content).unwrap(), attachment.url)
            })
            .collect()
    }

    #[test]
    fn files_in_a_pages_attachments_directory_become_its_attachments() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");

        let imported = import_into(
            &core,
            &grace,
            &area_id,
            &[
                ("Runbook.md", "![Flow](<Runbook/_attachments/Diagram (2).png>)\n\nSee [the log](<Runbook/_attachments/Deploy.md>)."),
                ("Runbook/_attachments/Diagram (2).png", "a diagram"),
                ("Runbook/_attachments/Deploy.md", "a log"),
                ("Runbook/_attachments/notes.txt", "kept though nothing links to it"),
                ("Runbook/Deploy.md", "A Page."),
            ],
        );

        let runbook = &imported.entries[0];
        assert_eq!(outline(&core, &grace, &area_id), ["Runbook", "  Deploy"]);
        let attachments = attachments_of(&core, &grace, &runbook.id);
        let names: Vec<_> = attachments.iter().map(|(name, content, _)| (name.as_str(), content.as_str())).collect();
        assert_eq!(names, [("Deploy.md", "a log"), ("Diagram (2).png", "a diagram"), ("notes.txt", "kept though nothing links to it")]);
        let url = |name: &str| attachments.iter().find(|(n, _, _)| n == name).unwrap().2.clone();
        assert_eq!(
            markdown_of(&core, &grace, &runbook.id),
            format!("![Flow](<{}>)\n\nSee [the log](<{}>).\n", url("Diagram (2).png"), url("Deploy.md"))
        );
    }

    #[test]
    fn files_linked_by_relative_path_become_attachments_of_the_first_page_that_links_to_them() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");

        let imported = import_into(
            &core,
            &grace,
            &area_id,
            &[
                ("Deploy.md", "![Flow](images/flow.png)"),
                ("Runbook.md", "![Flow](./images/flow.png) and [the spec](docs/spec.pdf)"),
                ("images/flow.png", "a diagram"),
                ("images/unused/old.png", "nobody refers to it"),
                ("docs/spec.pdf", "a spec"),
            ],
        );

        let (deploy, runbook) = (&imported.entries[0].id, &imported.entries[1].id);
        assert_eq!(outline(&core, &grace, &area_id), ["Deploy", "Runbook"]);
        let flow = attachments_of(&core, &grace, deploy);
        assert_eq!((flow[0].0.as_str(), flow[0].1.as_str()), ("flow.png", "a diagram"));
        let spec = attachments_of(&core, &grace, runbook);
        assert_eq!((spec.len(), spec[0].0.as_str(), spec[0].1.as_str()), (1, "spec.pdf", "a spec"));
        assert_eq!(markdown_of(&core, &grace, runbook), format!("![Flow](<{}>) and [the spec](<{}>)\n", flow[0].2, spec[0].2));
    }

    #[test]
    fn an_id_from_the_frontmatter_is_reused_only_when_nothing_has_it_yet() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let id = "0123456789abcdef0123456789abcdef";
        let file = format!("---\nid: \"{id}\"\nkind: page\ntitle: \"Runbook\"\n---\n");

        let first = import_into(&core, &grace, &area_id, &[("Runbook.md", &file)]);
        let second = import_into(&core, &grace, &area_id, &[("Runbook.md", &file)]);

        assert_eq!(first.entries[0].id, id);
        assert_ne!(second.entries[0].id, id);
        assert_eq!(outline(&core, &grace, &area_id), ["Runbook", "Runbook"]);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_reported_by_name_and_the_rest_is_imported() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let content = zip_of_bytes(&[("Guides/Broken.md", b"caf\xe9 au lait"), ("Runbook.md", b"Restart it.")]);

        let imported = import(&core, &grace, ToImport { area_id: area_id.clone(), parent_id: None, content }).unwrap().output;

        assert_eq!(outline(&core, &grace, &area_id), ["Runbook"]);
        let problems: Vec<_> = imported.problems.iter().map(|problem| problem.file.as_str()).collect();
        assert_eq!(problems, ["Guides/Broken.md"]);
        assert_eq!(imported.problems[0].message, "It is not Markdown in UTF-8, so it was left out.");
    }

    #[test]
    fn an_import_can_go_beneath_a_page_or_folder() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let folder = NewEntry { area_id: area_id.clone(), parent_id: None, title: "Archive".into() };
        let archive = tree::create_folder(&core, &grace, folder).unwrap().output.id;

        let content = zip_of(&[("Runbook.md", ""), ("Runbook/Deploy.md", "")]);
        import(&core, &grace, ToImport { area_id: area_id.clone(), parent_id: Some(archive), content }).unwrap();

        assert_eq!(outline(&core, &grace, &area_id), ["Archive (folder)", "  Runbook", "    Deploy"]);
    }

    #[test]
    fn only_someone_with_write_can_import_and_then_nothing_is_created() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area_id = area(&core, &grace, "Engineering");
        crate::ops::testing::give_role(&core, &area_id, &ken, crate::ops::permissions::Role::Read);

        let content = zip_of(&[("Runbook.md", "")]);
        let refused = import(&core, &ken, ToImport { area_id: area_id.clone(), parent_id: None, content: content.clone() }).unwrap_err();
        let hidden = import(&core, &ken, ToImport { area_id: "made-up".into(), parent_id: None, content }).unwrap_err();

        let empty = import(&core, &ken, ToImport { area_id: area_id.clone(), parent_id: None, content: zip_of(&[]) }).unwrap_err();

        assert_eq!(refused.kind(), "not_permitted");
        assert_eq!(empty.kind(), "not_permitted");
        assert_eq!(hidden.kind(), "not_found");
        assert!(outline(&core, &grace, &area_id).is_empty());
    }

    #[test]
    fn an_exported_area_imported_into_an_empty_area_has_the_same_tree_content_and_attachments() {
        use crate::ops::export::{export, ToExport};
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let original = area(&core, &grace, "Engineering");
        let folder = NewEntry { area_id: original.clone(), parent_id: None, title: "Guides #1".into() };
        let guides = tree::create_folder(&core, &grace, folder).unwrap().output.id;
        let new_page = |parent: Option<&str>, title: &str, markdown: &str| {
            let entry = NewEntry { area_id: original.clone(), parent_id: parent.map(Into::into), title: title.into() };
            tree::create_page(&core, &grace, tree::NewPage { entry, markdown: Some(markdown.into()) }).unwrap().output.id
        };
        let set_up = new_page(Some(&guides), "Set up", "- [x] Install\n- [ ] Configure\n");
        let runbook = new_page(None, "Runbook", "");
        new_page(Some(&runbook), "_attachments", "A Page named like the directory.");
        new_page(None, "notes", "");
        new_page(None, "Notes", "Same name, other case.");
        let content = base64::engine::general_purpose::STANDARD.encode("a diagram");
        let input = crate::ops::attachments::NewAttachment { page_id: runbook.clone(), name: "diagram.png".into(), content };
        let diagram = crate::ops::attachments::attach_file(&core, &grace, input).unwrap().output.id;
        let markdown = format!(
            "> [!NOTE]\n> Read [](turtle:{set_up}) first.\n\n![Flow](/api/attachments/{diagram})\n\n| A | B |\n| --- | --- |\n| :status[Live]{{color=green}} | x |\n"
        );
        crate::ops::content::replace_page(&core, &grace, crate::ops::content::NewContent { page_id: runbook.clone(), markdown }).unwrap();
        crate::ops::tags::add_tag(&core, &grace, crate::ops::tags::PageTag { page_id: runbook, tag: "Ops".into() }).unwrap();
        let exported = |area_id: &str| export(&core, &grace, ToExport { area_id: area_id.into(), entry_id: None }).unwrap().output.content;
        let copy = area(&core, &grace, "Copy");

        let imported = import(&core, &grace, ToImport { area_id: copy.clone(), parent_id: None, content: exported(&original) }).unwrap().output;

        assert!(imported.problems.is_empty());
        assert_eq!(outline(&core, &grace, &copy), outline(&core, &grace, &original));
        // The IDs are taken in this installation, so the copy has its own;
        // everything else reads the same.
        assert_eq!(without_ids(&exported(&copy)), without_ids(&exported(&original)));
    }

    /// The files of a ZIP in base64, by path, without their `id:` lines.
    fn without_ids(zip: &str) -> std::collections::BTreeMap<String, Vec<u8>> {
        use std::io::Read;
        let bytes = base64::engine::general_purpose::STANDARD.decode(zip).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        (0..archive.len())
            .map(|index| {
                let mut file = archive.by_index(index).unwrap();
                let mut content = Vec::new();
                file.read_to_end(&mut content).unwrap();
                let name = file.name().unwrap().to_string();
                if name.ends_with(".md") && !name.contains("/_attachments/") {
                    let text = String::from_utf8(content).unwrap();
                    content = text.lines().filter(|line| !line.starts_with("id: ")).collect::<Vec<_>>().join("\n").into_bytes();
                }
                (name, content)
            })
            .collect()
    }

    #[test]
    fn what_a_mac_adds_to_a_zip_is_skipped() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let content = zip_of_bytes(&[("Runbook.md", b"Hi"), ("__MACOSX/._Runbook.md", b"\x00\x05\x16\x07\xff"), (".DS_Store", b"\x00\xff")]);

        let imported = import(&core, &grace, ToImport { area_id: area_id.clone(), parent_id: None, content }).unwrap().output;

        assert_eq!(outline(&core, &grace, &area_id), ["Runbook"]);
        assert!(imported.problems.is_empty());
    }

    // What follows goes to `write_bundle` directly: it is the seam every
    // importer shares, whatever its source names things by.

    fn page(key: &str, title: &str, markdown: &str, links: &[(&str, Target)]) -> Incoming {
        let links = links.iter().map(|(address, target)| (address.to_string(), target.clone())).collect();
        Incoming {
            key: key.into(),
            kind: Kind::Page,
            title: title.into(),
            id: None,
            content: crate::markdown::from_markdown(markdown),
            tags: Vec::new(),
            links,
            attachments: Vec::new(),
            children: Vec::new(),
        }
    }

    #[test]
    fn a_bundle_links_by_the_keys_its_source_gives_even_to_what_comes_later() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let mut home = page("page:1", "Home", "See [it](https://wiki.example/x/2) and ![chart](chart.png).", &[
            ("https://wiki.example/x/2", Target::Entry("page:2".into())),
            ("chart.png", Target::Attachment("attachment:9".into())),
        ]);
        let mut later = page("page:2", "Later", "", &[]);
        later.attachments.push(IncomingAttachment { key: "attachment:9".into(), name: "chart.png".into(), content: b"a chart".to_vec() });
        home.children.push(later);

        let imported = write_bundle(&core, &grace, &area_id, None, Bundle { source: "a test".into(), entries: vec![home], problems: Vec::new() }, "test-import", &|_| {}).unwrap().imported;

        let later = &imported.entries[0].children[0].id;
        let chart = &attachments_of(&core, &grace, later)[0].2;
        assert_eq!(markdown_of(&core, &grace, &imported.entries[0].id), format!("See [Later](turtle:{later}) and ![chart](<{chart}>).\n"));
    }

    #[test]
    fn a_folder_holds_no_attachments_or_tags_so_they_are_told_and_the_rest_comes_in() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let mut guides = page("Guides", "Guides", "", &[]);
        guides.kind = Kind::Folder;
        guides.tags.push("handbook".into());
        guides.attachments.push(IncomingAttachment { key: "Guides/logo.png".into(), name: "logo.png".into(), content: b"a logo".to_vec() });
        guides.children.push(page("page:2", "Set up", "Install it.", &[]));

        let imported = write_bundle(&core, &grace, &area_id, None, Bundle { source: "a test".into(), entries: vec![guides], problems: Vec::new() }, "test-import", &|_| {}).unwrap().imported;

        assert_eq!(outline(&core, &grace, &area_id), ["Guides (folder)", "  Set up"]);
        let problems: Vec<_> = imported.problems.iter().map(|problem| (problem.file.as_str(), problem.message.as_str())).collect();
        assert_eq!(
            problems,
            [
                ("Guides/logo.png", "It belongs to a Folder, which holds no Attachments, so it was left out."),
                ("Guides", "Its Tag \"handbook\" was left out: a Folder has no Tags."),
            ]
        );
    }

    #[test]
    fn what_links_to_something_that_did_not_come_across_keeps_its_text_but_not_the_address() {
        let mut core = fresh();
        core.core.set_attachment_limit(4);
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let mut runbook = page("page:1", "Runbook", "![Flow](flow.png) See [the log](log.bin) and [Guides](folder:2).", &[
            ("flow.png", Target::Attachment("Runbook/flow.png".into())),
            ("log.bin", Target::Attachment("Runbook/log.bin".into())),
            ("folder:2", Target::Entry("folder:2".into())),
        ]);
        for name in ["flow.png", "log.bin"] {
            runbook.attachments.push(IncomingAttachment { key: format!("Runbook/{name}"), name: name.into(), content: b"too large".to_vec() });
        }
        let mut guides = page("folder:2", "Guides", "", &[]);
        guides.kind = Kind::Folder;

        let imported = write_bundle(&core, &grace, &area_id, None, Bundle { source: "a test".into(), entries: vec![runbook, guides], problems: Vec::new() }, "test-import", &|_| {}).unwrap().imported;

        assert_eq!(markdown_of(&core, &grace, &imported.entries[0].id), "*Not imported: flow.png* See the log and Guides.\n");
    }

    #[test]
    fn an_attachment_that_cannot_be_attached_is_reported_by_its_key_and_the_rest_is_written() {
        let mut core = fresh();
        core.core.set_attachment_limit(4);
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let mut runbook = page("Runbook.md", "Runbook", "Restart it.", &[]);
        runbook.attachments.push(IncomingAttachment { key: "Runbook/_attachments/big.bin".into(), name: "big.bin".into(), content: b"too large".to_vec() });
        runbook.attachments.push(IncomingAttachment { key: "Runbook/_attachments/ok.txt".into(), name: "ok.txt".into(), content: b"ok".to_vec() });

        let imported = write_bundle(&core, &grace, &area_id, None, Bundle { source: "a test".into(), entries: vec![runbook], problems: Vec::new() }, "test-import", &|_| {}).unwrap().imported;

        let runbook = &imported.entries[0].id;
        assert_eq!(markdown_of(&core, &grace, runbook), "Restart it.\n");
        let names: Vec<_> = attachments_of(&core, &grace, runbook).into_iter().map(|(name, _, _)| name).collect();
        assert_eq!(names, ["ok.txt"]);
        assert_eq!(imported.problems.len(), 1);
        assert_eq!(imported.problems[0].file, "Runbook/_attachments/big.bin");
        assert_eq!(
            imported.problems[0].message,
            "It is 9 bytes, larger than 4 bytes, the most this Turtle accepts for one file, so it was left out. Whoever runs Turtle can raise that limit with TURTLE_MAX_ATTACHMENT_MB."
        );
    }

    #[test]
    fn a_file_too_large_to_attach_is_told_by_its_size_rounded_up_and_the_setting_that_raises_the_limit() {
        let mut core = fresh();
        core.core.set_attachment_limit(25 * 1024 * 1024);

        let message = too_large_to_attach(&core, 25 * 1024 * 1024 + 1).unwrap();

        assert!(message.starts_with("It is 25.1 MB, larger than 25 MB,"), "{message}");
        assert!(message.ends_with("raise that limit with TURTLE_MAX_ATTACHMENT_MB."), "{message}");
        assert_eq!(too_large_to_attach(&core, 25 * 1024 * 1024), None);
    }

    #[test]
    fn writing_a_bundle_tells_how_many_pages_and_folders_are_created_and_then_how_many_contents_are_written() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Engineering");
        let mut runbook = page("Runbook.md", "Runbook", "Restart it.", &[]);
        runbook.children.push(page("Runbook/Deploy.md", "Deploy", "Ship it.", &[]));
        let steps = std::cell::RefCell::new(Vec::new());

        write_bundle(&core, &grace, &area_id, None, Bundle { source: "a test".into(), entries: vec![runbook, page("Empty.md", "Empty", "", &[])], problems: Vec::new() }, "test-import", &|step| {
            steps.borrow_mut().push(step)
        })
        .unwrap();

        assert_eq!(
            steps.into_inner(),
            [
                Step::Creating { done: 0, total: 3 },
                Step::Creating { done: 1, total: 3 },
                Step::Creating { done: 2, total: 3 },
                Step::Creating { done: 3, total: 3 },
                // The empty Page has no content to write.
                Step::Writing { done: 0, total: 2 },
                Step::Writing { done: 1, total: 2 },
                Step::Writing { done: 2, total: 2 },
            ]
        );
    }
}
