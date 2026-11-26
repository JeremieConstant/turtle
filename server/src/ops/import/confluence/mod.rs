//! Importing a Confluence space from its XML export, and checking first how
//! much of it comes across.
//!
//! The export is a ZIP holding `entities.xml`, every object of the space,
//! and `attachments/<page ID>/<attachment ID>/<version>`, each version of
//! each file. Only what is current comes across: a page's latest version,
//! not its history, drafts or what is in Confluence's trash. Each page's
//! content is converted from Confluence's storage format (`storage`), and
//! what was found in it is tallied for the report (`coverage`).

mod coverage;
mod dom;
mod entities;
mod storage;

use super::{check_target, write_bundle, Bundle, Incoming, IncomingAttachment, Problem};
use crate::ops::tree::{Entry, Kind};
use crate::ops::{Catalogue, Core, OpError, OpResult, Principal};
use coverage::{Coverage, NotImported, Tally};
use entities::Object;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read;

pub(in crate::ops) fn register(catalogue: &mut Catalogue) {
    catalogue.add(
        "check_confluence_import",
        "Reads a Confluence space exported as XML, a ZIP sent in base64, and tells how much of it would come across into Turtle, without importing anything: how many pages, folders and attachments, each kind of element and macro found in the pages with how often, on which pages and what it becomes (exactly, approximately, as a placeholder naming it, or left out), the share of elements that come across exactly, and what the space holds that is not imported, such as comments and page history.",
        check_confluence_import,
    );
    catalogue.add(
        "import_confluence",
        "Imports a Confluence space exported as XML, a ZIP sent in base64, into an Area or beneath one of its Pages or Folders: the page tree in order, folders as Folders, the content of each page's latest version, its attachments as Attachments, links between its pages as Page links, and global labels as Tags. Info, note, tip and warning macros become Callouts, code macros code blocks and status macros Statuses; a macro Turtle does not know becomes a placeholder naming it, followed by what it held. Comments, page history, drafts, permissions and personal labels are not imported. Answers with what was created, each file that did not come across, and the same report check_confluence_import gives. Needs the Write Role.",
        import_confluence,
    );
}

/// A Confluence space exported as XML.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfluenceExport {
    /// The ZIP Confluence exports a space as, in base64.
    pub content: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ToImportFromConfluence {
    pub area_id: String,
    /// The Page or Folder to import beneath; the top of the Area if absent.
    #[serde(default)]
    pub parent_id: Option<String>,
    /// The ZIP Confluence exports a space as, in base64.
    pub content: String,
}

/// What an import from Confluence created, and how much came across.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ImportedFromConfluence {
    /// The Pages and Folders created at the top of the import, with what is
    /// beneath them.
    pub entries: Vec<Entry>,
    /// Each page or attachment that did not come across, or not whole, with
    /// the reason.
    pub problems: Vec<super::Problem>,
    /// How much of the space came across.
    pub coverage: Coverage,
}

pub fn check_confluence_import(_core: &Core, principal: &Principal, input: ConfluenceExport) -> OpResult<Coverage> {
    crate::ops::areas::signed_in(principal)?;
    Ok(read_export(input.content)?.coverage.into())
}

pub fn import_confluence(core: &Core, principal: &Principal, input: ToImportFromConfluence) -> OpResult<ImportedFromConfluence> {
    // Access is checked before the ZIP is read, so a refused import reads nothing.
    check_target(core, principal, &input.area_id, input.parent_id.as_deref())?;
    let read = read_export(input.content)?;
    let imported = write_bundle(core, principal, &input.area_id, input.parent_id.as_deref(), read.bundle)?.output;
    Ok(ImportedFromConfluence { entries: imported.entries, problems: imported.problems, coverage: read.coverage }.into())
}

/// A space read from its export: what to create, and the report on it.
struct ReadExport {
    bundle: Bundle,
    coverage: Coverage,
}

/// The objects of `entities.xml` the import reads; every other class is
/// only counted.
const READ_CLASSES: [&str; 7] = ["Space", "Page", "Attachment", "Label", "Labelling", "ConfluenceUserImpl", "Comment"];

fn read_export(content: String) -> Result<ReadExport, OpError> {
    let bytes = crate::ops::attachments::decode_content(&content)?;
    drop(content);
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| OpError::InvalidInput("Choose a ZIP file.".into()))?;
    let prefix = export_prefix(&archive)?;
    let xml = {
        let mut file = archive.by_name(&format!("{prefix}entities.xml")).map_err(|_| unreadable())?;
        let mut xml = String::new();
        file.read_to_string(&mut xml).map_err(|_| unreadable())?;
        xml
    };
    let (objects, counts) = entities::read(&xml, |object| READ_CLASSES.contains(&object.class.as_str())).ok_or_else(unreadable)?;
    let space = Space::of(objects)?;
    let body_ids: HashSet<&str> = space.pages.iter().filter_map(|page| page.body_id.as_deref()).collect();
    let (bodies, _) = entities::read(&xml, |object| object.class == "BodyContent" && body_ids.contains(object.id.as_str()))
        .ok_or_else(unreadable)?;
    drop(xml);
    let bodies: HashMap<String, Object> = bodies.into_iter().map(|body| (body.id.clone(), body)).collect();
    Ok(space.into_bundle(&bodies, &counts, &mut archive, &prefix))
}

/// Where in the ZIP the export lies: `entities.xml` at the top, or in a
/// directory if the ZIP was made again around it.
fn export_prefix(archive: &zip::ZipArchive<std::io::Cursor<Vec<u8>>>) -> Result<String, OpError> {
    let names: Vec<String> = archive.file_names().filter_map(Result::ok).map(|name| name.into_owned()).collect();
    let found = names
        .iter()
        .filter_map(|name| name.as_str().strip_suffix("entities.xml").filter(|prefix| prefix.is_empty() || prefix.ends_with('/')))
        .min_by_key(|prefix| prefix.len());
    match found {
        Some(prefix) => Ok(prefix.to_string()),
        None if names.iter().any(|name| name.ends_with("content.csv") || name.ends_with("content.csv.gz")) => Err(OpError::InvalidInput(
            "This is Confluence's CSV export, which Turtle cannot read yet. Export the space as XML instead and choose that ZIP.".into(),
        )),
        None => Err(OpError::InvalidInput(
            "This ZIP is not a Confluence space exported as XML. In Confluence, export the space as XML and choose that ZIP.".into(),
        )),
    }
}

fn unreadable() -> OpError {
    OpError::InvalidInput("The export's entities.xml could not be read. Export the space from Confluence again and choose the new ZIP.".into())
}

/// The objects of the space that are imported, and how much is not.
struct Space {
    name: String,
    key: String,
    pages: Vec<Page>,
    attachments: Vec<Attachment>,
    /// The global labels of each page, by its ID.
    tags: HashMap<String, Vec<String>>,
    /// The name of each person, by their key.
    people: HashMap<String, String>,
    not_imported: Vec<NotImported>,
}

struct Page {
    id: String,
    title: String,
    parent: Option<String>,
    position: Option<i64>,
    /// The ID of its content; a folder has none.
    body_id: Option<String>,
}

struct Attachment {
    id: String,
    page_id: String,
    name: String,
    version: String,
}

/// Whether an object is the current version of what it is: not an earlier
/// one, a draft or in the trash.
fn is_current(object: &Object) -> bool {
    object.text("contentStatus").is_none_or(|status| status == "current") && !is_earlier_version(object)
}

fn is_earlier_version(object: &Object) -> bool {
    object.points_to("originalVersion").is_some() || object.text("originalVersionId").is_some_and(|id| !id.is_empty())
}

impl Space {
    fn of(objects: Vec<Object>) -> Result<Space, OpError> {
        let mut by_class: HashMap<&str, Vec<&Object>> = HashMap::new();
        for object in &objects {
            by_class.entry(object.class.as_str()).or_default().push(object);
        }
        let of = |class: &str| by_class.get(class).cloned().unwrap_or_default();
        let space = match of("Space").as_slice() {
            [space] => *space,
            [] => return Err(OpError::InvalidInput("This export holds no space. In Confluence, export a space as XML and choose that ZIP.".into())),
            _ => {
                return Err(OpError::InvalidInput(
                    "This export holds several spaces. In Confluence, export one space as XML and choose that ZIP.".into(),
                ))
            }
        };
        let in_space = |object: &Object| object.points_to("space").is_none_or(|id| id == space.id);
        let all_pages = of("Page");
        let pages: Vec<Page> = all_pages
            .iter()
            .filter(|page| is_current(page) && in_space(page))
            .map(|page| Page {
                id: page.id.clone(),
                title: page.text("title").unwrap_or_default().to_string(),
                parent: page.points_to("parent").map(str::to_string),
                position: page.text("position").and_then(|position| position.parse().ok()),
                body_id: page.collections.get("bodyContents").and_then(|ids| ids.first()).cloned(),
            })
            .collect();
        let page_ids: HashSet<&str> = pages.iter().map(|page| page.id.as_str()).collect();
        let all_attachments = of("Attachment");
        let attachments = all_attachments
            .iter()
            .filter(|attachment| is_current(attachment))
            .filter_map(|attachment| {
                let page_id = attachment.points_to("containerContent").or_else(|| attachment.points_to("content"))?;
                page_ids.contains(page_id).then(|| Attachment {
                    id: attachment.id.clone(),
                    page_id: page_id.to_string(),
                    name: attachment.text("title").unwrap_or_default().to_string(),
                    version: attachment.text("version").unwrap_or("1").to_string(),
                })
            })
            .collect();
        let labels: HashMap<&str, (&str, &str)> = of("Label")
            .iter()
            .map(|label| (label.id.as_str(), (label.text("name").unwrap_or_default(), label.text("namespace").unwrap_or_default())))
            .collect();
        let mut tags: HashMap<String, Vec<String>> = HashMap::new();
        let (mut personal_labels, mut other_labels) = (0, 0);
        for labelling in of("Labelling") {
            let (Some(page_id), Some((name, namespace))) =
                (labelling.points_to("content"), labelling.points_to("label").and_then(|id| labels.get(id)))
            else {
                continue;
            };
            if !page_ids.contains(page_id) {
                continue;
            }
            match *namespace {
                "global" => tags.entry(page_id.to_string()).or_default().push(name.to_string()),
                "my" => personal_labels += 1,
                _ => other_labels += 1,
            }
        }
        let people = of("ConfluenceUserImpl")
            .iter()
            .filter_map(|person| Some((person.id.clone(), person.text("name")?.to_string())))
            .collect();
        let status_count = |objects: &[&Object], status: &str| {
            objects.iter().filter(|object| in_space(object) && object.text("contentStatus") == Some(status) && !is_earlier_version(object)).count() as u32
        };
        let not_imported = vec![
            NotImported { what: "Comments".into(), count: of("Comment").iter().filter(|comment| is_current(comment)).count() as u32 },
            NotImported { what: "Earlier versions of pages".into(), count: all_pages.iter().filter(|page| is_earlier_version(page)).count() as u32 },
            NotImported { what: "Drafts".into(), count: status_count(&all_pages, "draft") },
            NotImported { what: "Archived pages".into(), count: status_count(&all_pages, "archived") },
            NotImported { what: "Pages in Confluence's trash".into(), count: status_count(&all_pages, "deleted") },
            NotImported {
                what: "Earlier versions of attachments".into(),
                count: all_attachments.iter().filter(|attachment| is_earlier_version(attachment)).count() as u32,
            },
            NotImported { what: "Personal labels".into(), count: personal_labels },
            NotImported { what: "Team labels and other labels that are not global".into(), count: other_labels },
        ];
        Ok(Space {
            name: space.text("name").unwrap_or_default().to_string(),
            key: space.text("key").unwrap_or_default().to_string(),
            pages,
            attachments,
            tags,
            people,
            not_imported,
        })
    }

    fn into_bundle(
        self,
        bodies: &HashMap<String, Object>,
        counts: &HashMap<String, u32>,
        archive: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>,
        prefix: &str,
    ) -> ReadExport {
        let mut lookup = storage::Lookup { key: self.key.clone(), people: self.people.clone(), ..storage::Lookup::default() };
        // A page is known by its title, which is unique in a space, so
        // problems name it as people know it; the ID tells apart any two
        // the export holds with one title all the same.
        let mut keys: HashMap<&str, String> = HashMap::new();
        for page in &self.pages {
            let taken = keys.values().any(|key| key == &page.title);
            keys.insert(&page.id, if taken { format!("{} ({})", page.title, page.id) } else { page.title.clone() });
            lookup.titles.insert(page.title.clone(), page.id.clone());
            if page.body_id.is_some() {
                lookup.pages.insert(page.id.clone(), keys[page.id.as_str()].clone());
            }
        }
        let mut problems = Vec::new();
        let mut attachments: HashMap<String, Vec<IncomingAttachment>> = HashMap::new();
        for attachment in &self.attachments {
            let key = format!("{}/{}", keys[attachment.page_id.as_str()], attachment.name);
            match read_attachment(archive, prefix, attachment) {
                Some(content) => {
                    lookup.attachments.insert((attachment.page_id.clone(), attachment.name.clone()), attachment.id.clone());
                    lookup.attachment_keys.insert(attachment.id.clone(), key.clone());
                    attachments.entry(attachment.page_id.clone()).or_default().push(IncomingAttachment { key, name: attachment.name.clone(), content });
                }
                None => problems.push(Problem::new(key, "Its file is not in the export, so it was left out.")),
            }
        }
        let attachment_count = attachments.values().map(Vec::len).sum::<usize>() as u32;
        let mut found = Vec::new();
        let mut incoming: HashMap<String, Incoming> = HashMap::new();
        for page in &self.pages {
            let kind = if page.body_id.is_some() { Kind::Page } else { Kind::Folder };
            let mut entry = Incoming {
                key: keys[page.id.as_str()].clone(),
                kind,
                title: page.title.clone(),
                id: None,
                content: Vec::new(),
                tags: self.tags.get(&page.id).cloned().unwrap_or_default(),
                links: HashMap::new(),
                attachments: attachments.remove(&page.id).unwrap_or_default(),
                children: Vec::new(),
            };
            if kind == Kind::Page {
                let body = page.body_id.as_ref().and_then(|id| bodies.get(id));
                let converted = match body {
                    Some(body) if body.text("bodyType").is_none_or(|kind| kind == STORAGE_FORMAT) => {
                        // The export writes `]]>` inside a body's CDATA as `]] >`.
                        let storage = body.text("body").unwrap_or_default().replace("]] >", "]]>");
                        storage::convert(&storage, &page.id, &lookup)
                    }
                    _ => None,
                };
                match converted {
                    Some(converted) => {
                        entry.content = converted.content;
                        entry.links = converted.links;
                        found.push((page.title.clone(), converted.found));
                    }
                    None => {
                        problems.push(Problem::new(keys[page.id.as_str()].clone(), "Its content could not be read, so the Page was created empty."));
                        let mut unread = Tally::default();
                        unread.add("unreadable page content");
                        found.push((page.title.clone(), unread));
                    }
                }
            }
            incoming.insert(page.id.clone(), entry);
        }
        let pages = self.pages.iter().filter(|page| page.body_id.is_some()).count() as u32;
        let folders = self.pages.len() as u32 - pages;
        let entries = tree_of(&self.pages, &mut incoming);
        let mut not_imported = self.not_imported;
        for (class, what) in [("SpacePermission", "Space permissions"), ("ContentPermissionSet", "Page restrictions"), ("BlogPost", "Blog posts")] {
            not_imported.push(NotImported { what: what.into(), count: counts.get(class).copied().unwrap_or(0) });
        }
        let coverage = coverage::report(self.name, (pages, folders, attachment_count), &found, not_imported);
        ReadExport { bundle: Bundle { entries, problems }, coverage }
    }
}

/// The `bodyType` of content in Confluence's storage format, the XHTML it
/// has written since version 4; older wiki markup has another.
const STORAGE_FORMAT: &str = "2";

/// The file of an attachment's current version: as its version names it,
/// or else the highest version there is.
fn read_attachment(archive: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, prefix: &str, attachment: &Attachment) -> Option<Vec<u8>> {
    let directory = format!("{prefix}attachments/{}/{}/", attachment.page_id, attachment.id);
    let exact = format!("{directory}{}", attachment.version);
    let name = if archive.index_for_name(&exact).is_some() {
        exact
    } else {
        archive
            .file_names()
            .filter_map(Result::ok)
            .filter_map(|name| Some((name.strip_prefix(&directory)?.parse::<u32>().ok()?, name.to_string())))
            .max()
            .map(|(_, name)| name)?
    };
    let mut file = archive.by_name(&name).ok()?;
    let mut content = Vec::new();
    file.read_to_end(&mut content).ok()?;
    Some(content)
}

/// The pages as a tree, each beneath its parent in the order Confluence
/// shows them: by position, and those without one by title. A page whose
/// parent is not imported stands at the top.
fn tree_of(pages: &[Page], incoming: &mut HashMap<String, Incoming>) -> Vec<Incoming> {
    let ids: HashSet<&str> = pages.iter().map(|page| page.id.as_str()).collect();
    let mut children: HashMap<Option<&str>, Vec<&Page>> = HashMap::new();
    for page in pages {
        let parent = page.parent.as_deref().filter(|parent| ids.contains(parent) && *parent != page.id);
        children.entry(parent).or_default().push(page);
    }
    for siblings in children.values_mut() {
        siblings.sort_by(|a, b| {
            (a.position.is_none(), a.position, a.title.to_lowercase()).cmp(&(b.position.is_none(), b.position, b.title.to_lowercase()))
        });
    }
    fn build(parent: Option<&str>, children: &HashMap<Option<&str>, Vec<&Page>>, incoming: &mut HashMap<String, Incoming>, depth: usize) -> Vec<Incoming> {
        // A parent chain that loops back is cut, rather than followed forever.
        if depth > 200 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for page in children.get(&parent).map(Vec::as_slice).unwrap_or_default() {
            if let Some(mut entry) = incoming.remove(&page.id) {
                entry.children = build(Some(&page.id), children, incoming, depth + 1);
                out.push(entry);
            }
        }
        out
    }
    build(None, &children, incoming, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    /// Prints the report on a real export, named by the environment
    /// variable `CONFLUENCE_EXPORT`, without importing it. Exports hold
    /// people's data and stay out of the repository, so this runs only when
    /// asked: `CONFLUENCE_EXPORT=space.zip scripts/cargo test --manifest-path
    /// server/Cargo.toml --bin turtle report_on_a_real_export -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn report_on_a_real_export() {
        let path = std::env::var("CONFLUENCE_EXPORT").expect("CONFLUENCE_EXPORT names the ZIP");
        let content = base64::engine::general_purpose::STANDARD.encode(std::fs::read(path).unwrap());
        let read = read_export(content).unwrap();
        println!("{}", serde_json::to_string_pretty(&read.coverage).unwrap());
        for problem in &read.bundle.problems {
            println!("problem: {}: {}", problem.file, problem.message);
        }
    }

    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::attachments::list_attachments;
    use crate::ops::content::{get_page_markdown, OfPage};
    use crate::ops::testing::{fresh, member};
    use crate::ops::tree::{get_page, get_tree, PageId, TreeOfArea};
    use std::io::Write;

    const PAGES: &str = "com.atlassian.confluence.pages";

    fn space(id: &str, key: &str, name: &str) -> String {
        format!(r#"<object class="Space" package="com.atlassian.confluence.spaces"><id name="id">{id}</id><property name="name"><![CDATA[{name}]]></property><property name="key"><![CDATA[{key}]]></property></object>"#)
    }

    /// A page of the space, current unless `status` or `original` say otherwise.
    struct Page<'a> {
        id: &'a str,
        title: &'a str,
        parent: Option<&'a str>,
        position: Option<&'a str>,
        body: Option<&'a str>,
        status: &'a str,
        original: Option<&'a str>,
    }

    fn page(id: &'static str, title: &'static str, parent: Option<&'static str>, position: Option<&'static str>, body: Option<&'static str>) -> Page<'static> {
        Page { id, title, parent, position, body, status: "current", original: None }
    }

    impl Page<'_> {
        fn xml(&self) -> String {
            let pointing = |name: &str, id: &str| format!(r#"<property name="{name}" class="Page" package="{PAGES}"><id name="id">{id}</id></property>"#);
            let mut xml = format!(r#"<object class="Page" package="{PAGES}"><id name="id">{}</id><property name="title"><![CDATA[{}]]></property>"#, self.id, self.title);
            if let Some(body) = self.body {
                xml += &format!(r#"<collection name="bodyContents" class="java.util.Collection"><element class="BodyContent" package="com.atlassian.confluence.core"><id name="id">{body}</id></element></collection>"#);
            }
            xml += &format!(r#"<property name="contentStatus"><![CDATA[{}]]></property>"#, self.status);
            match self.original {
                Some(original) => xml += &format!(r#"<property name="originalVersionId">{original}</property>"#),
                None => xml += r#"<property name="originalVersionId"/>"#,
            }
            xml += r#"<property name="space" class="Space" package="com.atlassian.confluence.spaces"><id name="id">1</id></property>"#;
            if let Some(position) = self.position {
                xml += &format!(r#"<property name="position">{position}</property>"#);
            }
            if let Some(parent) = self.parent {
                xml += &pointing("parent", parent);
            }
            xml + "</object>"
        }
    }

    /// The content of a page in storage format, written as the export
    /// writes it inside CDATA.
    fn body(id: &str, page_id: &str, storage: &str) -> String {
        format!(
            r#"<object class="BodyContent" package="com.atlassian.confluence.core"><id name="id">{id}</id><property name="body"><![CDATA[{}]]></property><property name="content" class="Page" package="{PAGES}"><id name="id">{page_id}</id></property><property name="bodyType">2</property></object>"#,
            storage.replace("]]>", "]] >")
        )
    }

    fn attachment(id: &str, page_id: &str, title: &str, version: &str, original: Option<&str>) -> String {
        let original = original.map(|id| format!("<property name=\"originalVersionId\">{id}</property>")).unwrap_or_default();
        format!(
            r#"<object class="Attachment" package="{PAGES}"><id name="id">{id}</id><property name="title"><![CDATA[{title}]]></property><property name="version">{version}</property>{original}<property name="contentStatus"><![CDATA[current]]></property><property name="containerContent" class="Page" package="{PAGES}"><id name="id">{page_id}</id></property></object>"#
        )
    }

    fn label(id: &str, name: &str, namespace: &str, page_id: &str) -> String {
        format!(
            r#"<object class="Label" package="com.atlassian.confluence.labels"><id name="id">{id}</id><property name="name"><![CDATA[{name}]]></property><property name="namespace"><![CDATA[{namespace}]]></property></object><object class="Labelling" package="com.atlassian.confluence.labels"><id name="id">{id}0</id><property name="label" class="Label" package="com.atlassian.confluence.labels"><id name="id">{id}</id></property><property name="content" class="Page" package="{PAGES}"><id name="id">{page_id}</id></property></object>"#
        )
    }

    /// An XML export ZIP, in base64, of these objects and files.
    fn export(objects: &[String], files: &[(&str, &[u8])]) -> String {
        let entities = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<hibernate-generic datetime=\"2026-11-26 11:19:52.169\">\n{}\n</hibernate-generic>", objects.join("\n"));
        let mut all: Vec<(&str, &[u8])> = vec![("entities.xml", entities.as_bytes()), ("exportDescriptor.properties", b"exportType=space\n")];
        all.extend_from_slice(files);
        zip_of(&all)
    }

    /// A ZIP of these files, by path, in base64.
    fn zip_of(files: &[(&str, &[u8])]) -> String {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (path, content) in files {
            zip.start_file(*path, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(content).unwrap();
        }
        base64::engine::general_purpose::STANDARD.encode(zip.finish().unwrap().into_inner())
    }

    /// A space "Knowledge Base" (KB): the home page, beneath it the folder
    /// "Guides" holding "Set up" and "Deploy" in that order, and "Contact".
    /// "Deploy" shows an image attached to it and links to "Set up" and to
    /// "Guides"; it has an earlier version, an earlier version of its image
    /// and two labels, one personal. "Set up" has a label too long for a
    /// Tag. There is also a draft and a page in the trash.
    fn knowledge_base() -> String {
        let deploy = "<h1>Deploy</h1><p>First read <ac:link><ri:page ri:content-title=\"Set up\" /></ac:link> in <ac:link><ri:page ri:content-title=\"Guides\" /></ac:link>.</p>\
            <ac:image><ri:attachment ri:filename=\"flow.png\" /></ac:image>\
            <ac:structured-macro ac:name=\"toc\" />\
            <ac:structured-macro ac:name=\"code\"><ac:plain-text-body><![CDATA[a]]]]><![CDATA[></b]]></ac:plain-text-body></ac:structured-macro>";
        let mut old_deploy = page("21", "Deploy", Some("3"), Some("2"), Some("210"));
        old_deploy.original = Some("20");
        let mut draft = page("40", "Unfinished", Some("1"), None, Some("400"));
        draft.status = "draft";
        let mut trashed = page("41", "Old", Some("1"), None, Some("410"));
        trashed.status = "deleted";
        export(
            &[
                space("1", "KB", "Knowledge Base"),
                page("2", "Home", None, None, Some("200")).xml(),
                page("3", "Guides", Some("2"), Some("1"), None).xml(),
                page("20", "Deploy", Some("3"), Some("7"), Some("201")).xml(),
                page("30", "Set up", Some("3"), Some("5"), Some("300")).xml(),
                page("31", "Contact", Some("2"), Some("9"), Some("301")).xml(),
                old_deploy.xml(),
                draft.xml(),
                trashed.xml(),
                body("200", "2", "<p>Welcome</p>"),
                body("201", "20", deploy),
                body("210", "21", "<p>Old</p>"),
                body("300", "30", "<p>Install it.</p>"),
                body("301", "31", "<p>Ask <ac:link><ri:user ri:userkey=\"u1\" /></ac:link>.</p>"),
                attachment("50", "20", "flow.png", "2", None),
                attachment("51", "20", "flow.png", "1", Some("50")),
                attachment("52", "20", "gone.pdf", "1", None),
                label("60", "runbook", "global", "20"),
                label("61", "favourite", "my", "20"),
                label("62", &"long".repeat(20), "global", "30"),
                r#"<object class="ConfluenceUserImpl" package="com.atlassian.confluence.user"><id name="key"><![CDATA[u1]]></id><property name="name"><![CDATA[grace]]></property></object>"#.into(),
            ],
            &[("attachments/20/50/1", b"old flow"), ("attachments/20/50/2", b"the flow")],
        )
    }

    fn area(core: &Core, admin: &Principal, name: &str) -> String {
        create_area(core, admin, NewArea { name: name.into() }).unwrap().output.id
    }

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
    fn a_space_comes_across_with_its_tree_content_attachments_links_and_tags() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Wiki");

        let imported = import_confluence(&core, &grace, ToImportFromConfluence { area_id: area_id.clone(), parent_id: None, content: knowledge_base() })
            .unwrap()
            .output;

        assert_eq!(outline(&core, &grace, &area_id), ["Home", "  Guides (folder)", "    Set up", "    Deploy", "  Contact"]);
        let home = &imported.entries[0];
        let (set_up, deploy) = (&home.children[0].children[0].id, &home.children[0].children[1].id);
        let flow = &list_attachments(&core, &grace, PageId { id: deploy.clone() }).unwrap().output.attachments;
        assert_eq!(flow.iter().map(|attachment| attachment.name.as_str()).collect::<Vec<_>>(), ["flow.png"]);
        assert_eq!(
            markdown_of(&core, &grace, deploy),
            format!("# Deploy\n\nFirst read [Set up](turtle:{set_up}) in Guides.\n\n![](<{}>)\n\n:confluence.toc[toc]\n\n```\na]]></b\n```\n", flow[0].url)
        );
        assert_eq!(markdown_of(&core, &grace, &home.children[1].id), "Ask @grace.\n");
        assert_eq!(get_page(&core, &grace, PageId { id: deploy.clone() }).unwrap().output.tags, ["runbook"]);
        let problems: Vec<_> = imported.problems.iter().map(|problem| (problem.file.as_str(), problem.message.as_str())).collect();
        assert_eq!(problems[0], ("Deploy/gone.pdf", "Its file is not in the export, so it was left out."));
        assert_eq!(problems[1].0, "Set up");
        assert!(problems[1].1.starts_with("Its Tag \"longlong"), "{}", problems[1].1);
        assert_eq!(problems.len(), 2);
    }

    #[test]
    fn the_report_tells_how_much_comes_across_and_what_is_not_imported() {
        let core = fresh();
        let grace = member(&core, "Grace", false);

        let coverage = check_confluence_import(&core, &grace, ConfluenceExport { content: knowledge_base() }).unwrap().output;

        assert_eq!(coverage.space, "Knowledge Base");
        assert_eq!((coverage.pages, coverage.folders, coverage.attachments), (4, 1, 1));
        // Home, Set up and Contact hold only paragraphs; Deploy holds a placeholder, and Contact a mention.
        assert_eq!(coverage.exact_pages, 2);
        let found: Vec<_> = coverage.elements.iter().map(|found| (found.name.as_str(), found.outcome, found.count)).collect();
        assert_eq!(
            found,
            [
                ("toc macro", coverage::Outcome::Placeholder, 1),
                ("link to a folder", coverage::Outcome::Approximate, 1),
                ("mention", coverage::Outcome::Approximate, 1),
                ("paragraph", coverage::Outcome::Exact, 4),
                ("code macro", coverage::Outcome::Exact, 1),
                ("heading", coverage::Outcome::Exact, 1),
                ("image", coverage::Outcome::Exact, 1),
                ("link to a page", coverage::Outcome::Exact, 1),
            ]
        );
        assert_eq!(coverage.elements[0].pages, ["Deploy"]);
        // 8 of 11 elements exactly, 10 of 11 exactly or approximately.
        assert_eq!((coverage.exact_percent, coverage.kept_percent), (72, 90));
        let not_imported: Vec<_> = coverage.not_imported.iter().map(|item| (item.what.as_str(), item.count)).collect();
        assert_eq!(
            not_imported,
            [("Earlier versions of pages", 1), ("Drafts", 1), ("Pages in Confluence's trash", 1), ("Earlier versions of attachments", 1), ("Personal labels", 1)]
        );
    }

    #[test]
    fn a_check_imports_nothing_and_needs_no_role() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area_id = area(&core, &grace, "Wiki");

        check_confluence_import(&core, &ken, ConfluenceExport { content: knowledge_base() }).unwrap();

        assert!(outline(&core, &grace, &area_id).is_empty());
    }

    #[test]
    fn only_someone_with_write_can_import_and_then_nothing_is_created() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area_id = area(&core, &grace, "Wiki");
        crate::ops::testing::give_role(&core, &area_id, &ken, crate::ops::permissions::Role::Read);

        let refused = import_confluence(&core, &ken, ToImportFromConfluence { area_id: area_id.clone(), parent_id: None, content: knowledge_base() }).unwrap_err();

        assert_eq!(refused.kind(), "not_permitted");
        assert!(outline(&core, &grace, &area_id).is_empty());
    }

    #[test]
    fn a_zip_that_is_not_an_xml_export_is_refused_with_what_to_do() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let message = |files: &[(&str, &[u8])]| {
            let content = zip_of(files);
            match check_confluence_import(&core, &grace, ConfluenceExport { content }).unwrap_err() {
                OpError::InvalidInput(message) => message,
                other => panic!("{other:?}"),
            }
        };

        assert_eq!(
            message(&[("content.csv.gz", b"contentid"), ("spaces.csv.gz", b"spaceid")]),
            "This is Confluence's CSV export, which Turtle cannot read yet. Export the space as XML instead and choose that ZIP."
        );
        assert_eq!(
            message(&[("Runbook.md", b"# Hi")]),
            "This ZIP is not a Confluence space exported as XML. In Confluence, export the space as XML and choose that ZIP."
        );
        assert_eq!(
            message(&[("entities.xml", b"<hibernate-generic><object class=\"Page\">")]),
            "The export's entities.xml could not be read. Export the space from Confluence again and choose the new ZIP."
        );
    }
}
