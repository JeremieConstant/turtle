//! Import: Pages, Folders and Attachments brought into an Area from outside.
//!
//! Every importer has two halves. A reader turns its source into a Bundle,
//! which looks the same whatever the source: `markdown` reads a ZIP of
//! Markdown files, as Turtle exports them (ADR-0011) or from anywhere else.
//! `write_bundle` then creates what a Bundle holds, the same for every
//! source: the tree in order, IDs kept where they are free, Attachments,
//! links pointed at what they name, Tags, and a report of what did not come
//! across instead of a failure. An importer for another source, such as
//! Confluence, is a new reader beside `markdown` and its own operation.

use super::tree::{self, Entry, Kind, NewEntry};
use super::{Catalogue, Core, OpError, OpResult, Principal};
use crate::model::{Block, Inline, Marks};
use std::collections::HashMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod markdown;

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue.add(
        "import",
        "Imports a ZIP of Markdown files, sent in base64, into an Area or beneath one of its Pages or Folders: an Export of Turtle's, or any folder of Markdown. Directories become the tree: a Markdown file is a Page, a directory of the same name holds what is beneath it, and a directory without one becomes a Folder. Frontmatter, where a file has it, gives the title, whether it is a page or a folder, the position among its siblings, the Tags and an ID, which is kept if nothing has it yet; without frontmatter a Page is titled after its file name. Relative links between the files become Page links, and files they refer to, and those in a Page's `_attachments` directory, become its Attachments. A file that cannot be imported is reported with the reason and the rest comes in. Needs the Write Role.",
        import,
    );
}

/// The largest ZIP that can be imported at once, unless whoever runs Turtle
/// sets another. An import is held in memory several times over while it is
/// read, so this is far below what a whole Confluence space may need.
pub const DEFAULT_IMPORT_LIMIT: u64 = 200 * 1024 * 1024;

/// Why a ZIP larger than the import limit is refused.
pub fn too_large(core: &Core) -> OpError {
    OpError::InvalidInput(format!(
        "This ZIP is larger than {}, the most this Turtle imports at once. Import it in parts, or ask whoever runs Turtle to raise the limit.",
        super::attachments::size_in_words(core.import_limit())
    ))
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
#[derive(Debug, Serialize, JsonSchema)]
pub struct Imported {
    /// The Pages and Folders created at the top of the import, with what is
    /// beneath them.
    pub entries: Vec<Entry>,
    /// Each file that did not come across, with the reason. Empty when all did.
    pub problems: Vec<Problem>,
}

/// A file that did not come across, and why.
#[derive(Debug, Serialize, JsonSchema)]
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
    write_bundle(core, principal, &input.area_id, input.parent_id.as_deref(), bundle)
}

/// What an importer read from its source, in a form that is the same
/// whatever the source: the Pages and Folders to create, in tree order.
pub(super) struct Bundle {
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
#[derive(Clone)]
pub(super) enum Target {
    /// The Page with this key, which becomes a Page link.
    Entry(String),
    /// The Attachment with this key, which becomes its address.
    Attachment(String),
}

/// Creates what the bundle holds beneath `parent_id`, or at the top of the Area.
pub(super) fn write_bundle(
    core: &Core,
    principal: &Principal,
    area_id: &str,
    parent_id: Option<&str>,
    bundle: Bundle,
) -> OpResult<Imported> {
    // Checked even with nothing to create, so an empty import is refused like any other.
    check_target(core, principal, area_id, parent_id)?;
    // Content is written once everything is created, so a link can point
    // to a Page that comes later in the tree.
    let mut created = Created { problems: bundle.problems, ..Created::default() };
    let entries = create_all(core, principal, area_id, parent_id, bundle.entries, &mut created)?;
    for (page_id, mut content, links) in std::mem::take(&mut created.content) {
        point_links(&mut content, &links, &created);
        super::content::write_content(core, principal, &page_id, content)?;
    }
    Ok(Imported { entries, problems: created.problems }.into())
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
fn point_links(content: &mut [Block], links: &HashMap<String, Target>, created: &Created) {
    super::export::for_each_inline(content, &mut |inline| match inline {
        Inline::Image { src, .. } => {
            if let Some(Target::Attachment(key)) = links.get(src.as_str()) {
                if let Some(url) = created.attachments.get(key) {
                    *src = url.clone();
                }
            }
        }
        Inline::Text { marks: Marks { link: Some(href), .. }, .. } => match links.get(href.as_str()) {
            Some(Target::Entry(key)) => {
                if let Some(page_id) = created.pages.get(key) {
                    *inline = Inline::PageLink { page_id: page_id.clone() };
                }
            }
            Some(Target::Attachment(key)) => {
                if let Some(url) = created.attachments.get(key) {
                    *href = url.clone();
                }
            }
            None => {}
        },
        _ => {}
    });
}

fn create_all(
    core: &Core,
    principal: &Principal,
    area_id: &str,
    parent_id: Option<&str>,
    incoming: Vec<Incoming>,
    created_so_far: &mut Created,
) -> Result<Vec<Entry>, OpError> {
    let mut entries = Vec::new();
    for incoming in incoming {
        let entry = NewEntry { area_id: area_id.into(), parent_id: parent_id.map(Into::into), title: incoming.title };
        let mut created = tree::create_with_id(core, principal, entry, incoming.kind, incoming.id.as_deref())?.output;
        for attachment in incoming.attachments {
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
            created_so_far.pages.insert(incoming.key, created.id.clone());
            if !incoming.content.is_empty() {
                created_so_far.content.push((created.id.clone(), incoming.content, incoming.links));
            }
        }
        for tag in incoming.tags {
            super::tags::add_tag(core, principal, super::tags::PageTag { page_id: created.id.clone(), tag })?;
        }
        created.children = create_all(core, principal, area_id, Some(&created.id), incoming.children, created_so_far)?;
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

        let imported = write_bundle(&core, &grace, &area_id, None, Bundle { entries: vec![home], problems: Vec::new() }).unwrap().output;

        let later = &imported.entries[0].children[0].id;
        let chart = &attachments_of(&core, &grace, later)[0].2;
        assert_eq!(markdown_of(&core, &grace, &imported.entries[0].id), format!("See [Later](turtle:{later}) and ![chart](<{chart}>).\n"));
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

        let imported = write_bundle(&core, &grace, &area_id, None, Bundle { entries: vec![runbook], problems: Vec::new() }).unwrap().output;

        let runbook = &imported.entries[0].id;
        assert_eq!(markdown_of(&core, &grace, runbook), "Restart it.\n");
        let names: Vec<_> = attachments_of(&core, &grace, runbook).into_iter().map(|(name, _, _)| name).collect();
        assert_eq!(names, ["ok.txt"]);
        assert_eq!(imported.problems.len(), 1);
        assert_eq!(imported.problems[0].file, "Runbook/_attachments/big.bin");
        assert!(imported.problems[0].message.starts_with("This file is larger than"), "{}", imported.problems[0].message);
    }
}
