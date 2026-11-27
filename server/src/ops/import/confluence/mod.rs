//! Importing a Confluence space from its export, and checking first how
//! much of it comes across.
//!
//! Confluence exports a space as a ZIP in one of two forms: XML, with
//! `entities.xml` holding every object of the space (`xml`), or CSV, with
//! one table per kind of object (`csv`). Both hold the attachments as
//! `attachments/<page ID>/<attachment ID>/<version>`, each version of each
//! file. Each is read into the same `Space`, and from there on the import
//! is the same: only what is current comes across, a page's latest version
//! and not its history, drafts or what is in Confluence's trash. Each
//! page's content is converted from Confluence's storage format (`storage`),
//! and what was found in it is tallied for the report (`coverage`).

mod coverage;
mod csv;
mod dom;
mod entities;
mod report;
mod storage;
mod xml;

pub use coverage::Coverage;

use super::running::{self, ImportProgress};
use super::{check_target, too_large_to_attach, write_bundle, Bundle, Incoming, IncomingAttachment, Problem};
use crate::ops::tree::{Entry, Kind};
use crate::ops::{Catalogue, Core, OpError, OpResult, Principal};
use coverage::{NotImported, Tally, TooLarge};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read;

pub(in crate::ops) fn register(catalogue: &mut Catalogue) {
    catalogue.add(
        "check_confluence_import",
        "Reads a Confluence space exported as CSV or XML, a ZIP sent in base64, and tells how much of it would come across into Turtle, without importing anything: how many pages, folders and attachments, each kind of element and macro found in the pages with how often, on which pages and what it becomes (exactly, approximately, as a placeholder naming it, or left out), the share of elements that come across exactly, what the space holds that is not imported, such as comments and page history, and which attachments are larger than this Turtle accepts, which whoever runs it can raise with TURTLE_MAX_ATTACHMENT_MB. Mentions are matched with the Members and Guests the caller may see.",
        check_confluence_import,
    );
    catalogue.add(
        "import_confluence",
        "Imports a Confluence space exported as CSV or XML, a ZIP sent in base64, into an Area or beneath one of its Pages or Folders: the page tree in order, folders as Folders, the content of each page's latest version, its attachments as Attachments, links between its pages as Page links, mentions of people as Mentions of the Members and Guests with the same e-mail address, and global labels as Tags. Info, note, tip and warning macros become Callouts, code macros code blocks and status macros Statuses; a macro Turtle does not know becomes a placeholder naming it, followed by what it held. Comments, page history, drafts, permissions and personal labels are not imported. Answers with what was created, each file that did not come across, and the same report check_confluence_import gives. Needs the Write Role.",
        import_confluence,
    );
    catalogue.add(
        "start_import_confluence",
        "Starts what import_confluence does and answers at once, before anything is read, with the import's ID; get_import_progress then tells how far it has got, the report on the space as soon as it is read, and, once it is done, what was created and what did not come across. Meant for whole spaces, which take a while to bring in. Needs the Write Role.",
        start_import_confluence,
    );
}

/// A Confluence space exported as CSV or XML.
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
    /// The import as list_imports names it, to undo it with undo_import.
    pub import_id: String,
    /// The Pages and Folders created at the top of the import, with what is
    /// beneath them; the report Page is apart.
    pub entries: Vec<Entry>,
    /// The Page holding the report on the import, beside what was imported;
    /// absent if it could not be created, which `problems` then tells.
    pub report_page_id: Option<String>,
    /// Each page or attachment that did not come across, or not whole, with
    /// the reason.
    pub problems: Vec<super::Problem>,
    /// How much of the space came across.
    pub coverage: Coverage,
}

pub fn check_confluence_import(core: &Core, principal: &Principal, input: ConfluenceExport) -> OpResult<Coverage> {
    crate::ops::areas::signed_in(principal)?;
    Ok(read_export(core, principal, input.content)?.coverage.into())
}

pub fn import_confluence(core: &Core, principal: &Principal, input: ToImportFromConfluence) -> OpResult<ImportedFromConfluence> {
    // Access is checked before the ZIP is read, so a refused import reads nothing.
    check_target(core, principal, &input.area_id, input.parent_id.as_deref())?;
    let read = read_export(core, principal, input.content)?;
    let coverage = read.coverage.clone();
    let import_id = super::record::new_id()?;
    let imported = write(core, principal, &input.area_id, input.parent_id.as_deref(), read, &import_id, &|_| {})?;
    Ok(ImportedFromConfluence { import_id, entries: imported.entries, report_page_id: imported.report_page_id, problems: imported.problems, coverage }.into())
}

pub fn start_import_confluence(core: &Core, principal: &Principal, input: ToImportFromConfluence) -> OpResult<ImportProgress> {
    // A refused import is refused at once, not once it is running.
    check_target(core, principal, &input.area_id, input.parent_id.as_deref())?;
    running::start(core, principal, move |core, principal, following| {
        let read = read_export(core, principal, input.content)?;
        following.read(&read.coverage);
        let step = |step| following.step(step);
        write(core, principal, &input.area_id, input.parent_id.as_deref(), read, following.import_id(), &step)
    })
}

/// Creates what was read, then the report on it as a Page beside it.
fn write(
    core: &Core,
    principal: &Principal,
    area_id: &str,
    parent_id: Option<&str>,
    read: ReadExport,
    import_id: &str,
    progress: super::Progress,
) -> Result<super::Imported, OpError> {
    let ReadExport { bundle, coverage, found } = read;
    let written = write_bundle(core, principal, area_id, parent_id, bundle, import_id, progress)?;
    let pages: Vec<report::Checked> = found
        .into_iter()
        .filter_map(|found| Some(report::Checked { page_id: written.pages.get(&found.key)?.clone(), path: found.path, tally: found.tally }))
        .collect();
    let date = super::record::date_of(core.now());
    let markdown = report::markdown(&coverage, &pages, &written.imported.problems, &date);
    let entry = crate::ops::tree::NewEntry { area_id: area_id.into(), parent_id: parent_id.map(Into::into), title: report::title(&coverage, &date) };
    let mut imported = written.imported;
    // Everything is created by now: a report that cannot be is told, and the import stands.
    match crate::ops::tree::create_page(core, principal, crate::ops::tree::NewPage { entry, markdown: Some(markdown) }) {
        Ok(page) => {
            super::record::created(core, import_id, &page.output.id)?;
            super::record::report(core, import_id, &page.output.id)?;
            imported.report_page_id = Some(page.output.id);
        }
        Err(OpError::InvalidInput(message) | OpError::NotPermitted(message)) => {
            imported.problems.push(super::Problem::new("Import report", format!("The report could not be created as a Page: {message}")));
        }
        Err(error) => return Err(error),
    }
    Ok(imported)
}

/// What one page's content held, and where the page is, for the report.
struct FoundOnPage {
    /// The page's key in the Bundle.
    key: String,
    /// The titles of the pages and folders above it, from the top down.
    path: Vec<String>,
    tally: Tally,
}

/// A space read from its export: what to create, and the report on it.
struct ReadExport {
    bundle: Bundle,
    coverage: Coverage,
    /// What each page's content held, in the order the pages were read.
    found: Vec<FoundOnPage>,
}

/// A Confluence export, held in memory as it was sent.
type Archive = zip::ZipArchive<std::io::Cursor<Vec<u8>>>;

fn read_export(core: &Core, principal: &Principal, content: String) -> Result<ReadExport, OpError> {
    // Whom a mention can name: the Members and Guests the importer may see,
    // by e-mail address in lower case.
    let people_in_turtle: HashMap<String, String> = crate::ops::profiles::list_profiles(core, principal, crate::ops::accounts::Nothing {})?
        .output
        .profiles
        .into_iter()
        .map(|profile| (profile.email.to_lowercase(), profile.id))
        .collect();
    let bytes = crate::ops::attachments::decode_content(&content)?;
    drop(content);
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| OpError::InvalidInput("Choose a ZIP file.".into()))?;
    let (space, prefix) = match export_in(&archive)? {
        Export::Xml(prefix) => (xml::read(&mut archive, &prefix)?, prefix),
        Export::Csv(prefix) => (csv::read(&mut archive, &prefix)?, prefix),
    };
    Ok(space.into_bundle(core, people_in_turtle, &mut archive, &prefix))
}

/// Which form an export is in, and where in the ZIP it lies: at the top,
/// or in a directory if the ZIP was made again around it.
enum Export {
    Xml(String),
    Csv(String),
}

fn export_in(archive: &Archive) -> Result<Export, OpError> {
    let names: Vec<String> = archive.file_names().filter_map(Result::ok).map(|name| name.into_owned()).collect();
    let prefix_of = |file: &str| {
        names
            .iter()
            .filter_map(|name| name.as_str().strip_suffix(file).filter(|prefix| prefix.is_empty() || prefix.ends_with('/')))
            .min_by_key(|prefix| prefix.len())
            .map(str::to_string)
    };
    if let Some(prefix) = prefix_of("entities.xml") {
        return Ok(Export::Xml(prefix));
    }
    if let Some(prefix) = prefix_of("content.csv.gz").or_else(|| prefix_of("content.csv")) {
        return Ok(Export::Csv(prefix));
    }
    Err(OpError::InvalidInput(
        "This ZIP is not a Confluence space export. In Confluence, export the space as CSV or XML and choose that ZIP.".into(),
    ))
}

/// The one space an export holds.
fn the_one_space<T>(spaces: &[T]) -> Result<&T, OpError> {
    match spaces {
        [space] => Ok(space),
        [] => Err(no_space()),
        _ => Err(several_spaces()),
    }
}

/// The marker of content in Confluence's storage format, the XHTML it has
/// written since version 4, among the kinds of content an export holds
/// (`bodyType` in XML, `bodytypeid` in CSV); older wiki markup has another.
const STORAGE_FORMAT: &str = "2";

fn no_space() -> OpError {
    OpError::InvalidInput("This export holds no space. In Confluence, export a space and choose that ZIP.".into())
}

fn several_spaces() -> OpError {
    OpError::InvalidInput("This export holds several spaces. In Confluence, export one space and choose that ZIP.".into())
}

/// A space as its export holds it, whatever the export's form: what is
/// imported, and how much is not.
struct Space {
    name: String,
    key: String,
    pages: Vec<Page>,
    attachments: Vec<Attachment>,
    /// The global labels of each page, by its ID.
    tags: HashMap<String, Vec<String>>,
    /// The name of each person, by their key.
    people: HashMap<String, String>,
    /// The e-mail address of each person whose export tells it, by their key.
    emails: HashMap<String, String>,
    not_imported: Unimported,
}

/// What a space holds that is not imported, as far as its export tells.
#[derive(Default)]
struct Unimported {
    comments: u32,
    earlier_versions_of_pages: u32,
    drafts: u32,
    archived_pages: u32,
    pages_in_trash: u32,
    earlier_versions_of_attachments: u32,
    personal_labels: u32,
    other_labels: u32,
    space_permissions: u32,
    page_restrictions: u32,
    blog_posts: u32,
    whiteboards: u32,
    databases: u32,
}

impl Unimported {
    /// As the report lists it; what the space holds none of is left out there.
    fn listed(self) -> Vec<NotImported> {
        [
            ("Comments", self.comments),
            ("Earlier versions of pages", self.earlier_versions_of_pages),
            ("Drafts", self.drafts),
            ("Archived pages", self.archived_pages),
            ("Pages in Confluence's trash", self.pages_in_trash),
            ("Earlier versions of attachments", self.earlier_versions_of_attachments),
            ("Personal labels", self.personal_labels),
            ("Team labels and other labels that are not global", self.other_labels),
            ("Space permissions", self.space_permissions),
            ("Page restrictions", self.page_restrictions),
            ("Blog posts", self.blog_posts),
            ("Whiteboards", self.whiteboards),
            ("Databases", self.databases),
        ]
        .into_iter()
        .map(|(what, count)| NotImported { what: what.into(), count })
        .collect()
    }
}

/// A page or folder of the space, in its latest version.
struct Page {
    id: String,
    title: String,
    parent: Option<String>,
    position: Option<i64>,
    kind: Kind,
    /// A page's content in the storage format; nothing for a folder, or for
    /// a page whose content is in another format or not in the export.
    storage: Option<String>,
}

struct Attachment {
    id: String,
    page_id: String,
    name: String,
    version: String,
}

/// A label on a page, as an export tells it.
struct Labelling<'a> {
    page_id: &'a str,
    name: &'a str,
    /// `global` for a label of the space, `my` for someone's own.
    namespace: &'a str,
}

/// The labels of a space: its global labels as each page's Tags, and how
/// many labels are not global and so not imported.
struct Labels {
    tags: HashMap<String, Vec<String>>,
    personal: u32,
    other: u32,
}

impl Labels {
    /// The labels of the pages and folders imported.
    fn of<'a>(labelled: impl Iterator<Item = Labelling<'a>>, imported: &HashSet<&str>) -> Labels {
        let mut labels = Labels { tags: HashMap::new(), personal: 0, other: 0 };
        for labelling in labelled.filter(|labelling| imported.contains(labelling.page_id)) {
            match labelling.namespace {
                "global" => labels.tags.entry(labelling.page_id.to_string()).or_default().push(labelling.name.to_string()),
                "my" => labels.personal += 1,
                _ => labels.other += 1,
            }
        }
        labels
    }
}

impl Space {
    fn into_bundle(self, core: &Core, people_in_turtle: HashMap<String, String>, archive: &mut Archive, prefix: &str) -> ReadExport {
        let mut lookup = storage::Lookup {
            key: self.key.clone(),
            people: self.people.clone(),
            emails: self.emails.clone(),
            people_in_turtle,
            ..storage::Lookup::default()
        };
        // A page is known by its title, which is unique in a space, so
        // problems name it as people know it; the ID tells apart any two
        // the export holds with one title all the same.
        let mut keys: HashMap<&str, String> = HashMap::new();
        for page in &self.pages {
            let taken = keys.values().any(|key| key == &page.title);
            keys.insert(&page.id, if taken { format!("{} ({})", page.title, page.id) } else { page.title.clone() });
            // A page and a folder may share a title; a link by title is to the page.
            if page.kind == Kind::Page || !lookup.titles.contains_key(&page.title) {
                lookup.titles.insert(page.title.clone(), page.id.clone());
            }
            if page.kind == Kind::Page {
                lookup.pages.insert(page.id.clone(), keys[page.id.as_str()].clone());
            }
        }
        let mut problems = Vec::new();
        let mut attachments: HashMap<String, Vec<IncomingAttachment>> = HashMap::new();
        let mut too_large = Vec::new();
        for attachment in &self.attachments {
            let key = format!("{}/{}", keys[attachment.page_id.as_str()], attachment.name);
            match read_attachment(core, archive, prefix, attachment) {
                AttachmentFile::Found(content) => {
                    lookup.attachments.insert((attachment.page_id.clone(), attachment.name.clone()), attachment.id.clone());
                    lookup.attachment_keys.insert(attachment.id.clone(), key.clone());
                    attachments.entry(attachment.page_id.clone()).or_default().push(IncomingAttachment { key, name: attachment.name.clone(), content });
                }
                // Told before anything is created, rather than found once
                // the import comes to it.
                AttachmentFile::TooLarge { bytes, message } => {
                    too_large.push(TooLarge { file: key.clone(), bytes });
                    problems.push(Problem::new(key, message));
                }
                AttachmentFile::Missing => problems.push(Problem::new(key, "Its file is not in the export, so it was left out.")),
            }
        }
        let attachment_count = attachments.values().map(Vec::len).sum::<usize>() as u32;
        let mut found = Vec::new();
        let mut incoming: HashMap<String, Incoming> = HashMap::new();
        for page in &self.pages {
            let kind = page.kind;
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
                let converted = page.storage.as_deref().and_then(|storage| storage::convert(storage, &page.id, &lookup));
                match converted {
                    Some(converted) => {
                        entry.content = converted.content;
                        entry.links = converted.links;
                        found.push((page.id.clone(), keys[page.id.as_str()].clone(), page.title.clone(), converted.found));
                    }
                    None => {
                        problems.push(Problem::new(keys[page.id.as_str()].clone(), "Its content could not be read, so the Page was created empty."));
                        let mut unread = Tally::default();
                        unread.add("unreadable page content");
                        found.push((page.id.clone(), keys[page.id.as_str()].clone(), page.title.clone(), unread));
                    }
                }
            }
            incoming.insert(page.id.clone(), entry);
        }
        let pages = self.pages.iter().filter(|page| page.kind == Kind::Page).count() as u32;
        let folders = self.pages.len() as u32 - pages;
        let entries = tree_of(&self.pages, &mut incoming);
        let by_title: Vec<(String, Tally)> = found.iter().map(|(_, _, title, tally)| (title.clone(), tally.clone())).collect();
        let source = format!("the Confluence space {}", self.name);
        let mut coverage = coverage::report(self.name, (pages, folders, attachment_count), &by_title, self.not_imported.listed());
        coverage.attachment_limit_bytes = core.attachment_limit();
        coverage.too_large = too_large;
        let parents: HashMap<&str, (&str, Option<&str>)> =
            self.pages.iter().map(|page| (page.id.as_str(), (page.title.as_str(), page.parent.as_deref()))).collect();
        let found = found
            .into_iter()
            .map(|(id, key, _, tally)| FoundOnPage { key, path: path_to(&parents, &id), tally })
            .collect();
        ReadExport { bundle: Bundle { source, entries, problems }, coverage, found }
    }
}

/// What became of reading an attachment's file.
enum AttachmentFile {
    Found(Vec<u8>),
    /// Larger than this Turtle accepts, so it is not read at all.
    TooLarge { bytes: u64, message: String },
    Missing,
}

/// The titles of the pages and folders above a page, from the top down,
/// given each one's title and parent by ID.
fn path_to(parents: &HashMap<&str, (&str, Option<&str>)>, id: &str) -> Vec<String> {
    let mut path = Vec::new();
    let mut at = parents.get(id).and_then(|(_, parent)| *parent);
    // A parent chain that loops back is cut, rather than followed forever.
    while let Some(parent) = at.filter(|_| path.len() < 200) {
        let Some((title, above)) = parents.get(parent) else { break };
        path.push(title.to_string());
        at = *above;
    }
    path.reverse();
    path
}

/// The file of an attachment's current version: as its version names it,
/// or else the highest version there is.
fn read_attachment(core: &Core, archive: &mut Archive, prefix: &str, attachment: &Attachment) -> AttachmentFile {
    let Some(name) = attachment_file(archive, prefix, attachment) else {
        return AttachmentFile::Missing;
    };
    let Ok(mut file) = archive.by_name(&name) else {
        return AttachmentFile::Missing;
    };
    if let Some(message) = too_large_to_attach(core, file.size()) {
        return AttachmentFile::TooLarge { bytes: file.size(), message };
    }
    let mut content = Vec::new();
    match file.read_to_end(&mut content) {
        Ok(_) => AttachmentFile::Found(content),
        Err(_) => AttachmentFile::Missing,
    }
}

/// The name in the ZIP of an attachment's current version's file.
fn attachment_file(archive: &Archive, prefix: &str, attachment: &Attachment) -> Option<String> {
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
    Some(name)
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
        let core = crate::ops::testing::fresh();
        let reader = crate::ops::testing::member(&core, "Reader", false);
        let read = read_export(&core, &reader, content).unwrap();
        println!("{}", serde_json::to_string_pretty(&read.coverage).unwrap());
        for problem in &read.bundle.problems {
            println!("problem: {}: {}", problem.file, problem.message);
        }
    }

    /// Imports a real export, named by `CONFLUENCE_EXPORT`, into a fresh
    /// Turtle and prints how long each stage took: a way to try a large
    /// space. Run it as `report_on_a_real_export`, with `--release`.
    #[test]
    #[ignore]
    fn import_a_real_export() {
        let path = std::env::var("CONFLUENCE_EXPORT").expect("CONFLUENCE_EXPORT names the ZIP");
        let started = std::time::Instant::now();
        let content = base64::engine::general_purpose::STANDARD.encode(std::fs::read(path).unwrap());
        let core = crate::ops::testing::fresh();
        let writer = crate::ops::testing::member(&core, "Writer", false);
        let area_id = create_area(&core, &writer, NewArea { name: "Imported".into() }).unwrap().output.id;
        println!("encoded after {:?}", started.elapsed());
        let read = read_export(&core, &writer, content).unwrap();
        println!("read after {:?}: {} Pages, {} Folders, {} Attachments", started.elapsed(), read.coverage.pages, read.coverage.folders, read.coverage.attachments);
        let stage = std::cell::Cell::new(0);
        let step = |step| {
            let now = match step {
                super::super::Step::Creating { .. } => 1,
                super::super::Step::Writing { .. } => 2,
            };
            if stage.replace(now) != now {
                println!("stage {now} begins after {:?}", started.elapsed());
            }
        };
        let imported = write(&core, &writer, &area_id, None, read, "i1", &step).unwrap();
        println!("imported after {:?}, {} problems", started.elapsed(), imported.problems.len());
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
                body("301", "31", "<p>Ask <ac:link><ri:user ri:userkey=\"u1\" /></ac:link> or <ac:link><ri:user ri:userkey=\"u2\" /></ac:link>.</p>"),
                attachment("50", "20", "flow.png", "2", None),
                attachment("51", "20", "flow.png", "1", Some("50")),
                attachment("52", "20", "gone.pdf", "1", None),
                label("60", "runbook", "global", "20"),
                label("61", "favourite", "my", "20"),
                label("62", &"long".repeat(20), "global", "30"),
                // Confluence Cloud names a person by their e-mail address.
                r#"<object class="ConfluenceUserImpl" package="com.atlassian.confluence.user"><id name="key"><![CDATA[u1]]></id><property name="name"><![CDATA[Member-Grace@Example.org]]></property></object>"#.into(),
                r#"<object class="ConfluenceUserImpl" package="com.atlassian.confluence.user"><id name="key"><![CDATA[u2]]></id><property name="name"><![CDATA[ada@elsewhere.example]]></property></object>"#.into(),
            ],
            &[("attachments/20/50/1", b"old flow"), ("attachments/20/50/2", b"the flow")],
        )
    }

    /// A table of a CSV export with these columns and rows.
    fn table(columns: &[&str], rows: &[Vec<&str>]) -> Vec<u8> {
        let mut writer = ::csv::Writer::from_writer(Vec::new());
        writer.write_record(columns).unwrap();
        for row in rows {
            writer.write_record(row).unwrap();
        }
        writer.into_inner().unwrap()
    }

    fn gzipped(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    /// The space `knowledge_base` describes, exported as CSV: the same pages,
    /// a folder that is one, attachments, labels and earlier versions. One
    /// table is gzipped as its name says; the others are not, as Confluence
    /// often writes them.
    fn knowledge_base_csv() -> String {
        let deploy = "<h1>Deploy</h1><p>First read <ac:link><ri:page ri:content-title=\"Set up\" /></ac:link> in <ac:link><ri:page ri:content-title=\"Guides\" /></ac:link>.</p>\
            <ac:image><ri:attachment ri:filename=\"flow.png\" /></ac:image>\
            <ac:structured-macro ac:name=\"toc\" />\
            <ac:structured-macro ac:name=\"code\"><ac:plain-text-body><![CDATA[a]]]]><![CDATA[></b]]></ac:plain-text-body></ac:structured-macro>";
        let columns = ["contentid", "contenttype", "title", "version", "prevver", "content_status", "pageid", "spaceid", "child_position", "parentid"];
        let content = table(
            &columns,
            &[
                vec!["2", "PAGE", "Home", "1", "", "current", "", "1", "", ""],
                vec!["3", "FOLDER", "Guides", "1", "", "current", "", "1", "1", "2"],
                vec!["20", "PAGE", "Deploy", "2", "", "current", "", "1", "7", "3"],
                vec!["21", "PAGE", "Deploy", "1", "20", "current", "", "1", "2", "3"],
                vec!["30", "PAGE", "Set up", "1", "", "current", "", "1", "5", "3"],
                vec!["31", "PAGE", "Contact", "1", "", "current", "", "1", "9", "2"],
                vec!["40", "PAGE", "Unfinished", "1", "", "draft", "", "1", "", "2"],
                vec!["41", "PAGE", "Old", "1", "", "deleted", "", "1", "", "2"],
                vec!["50", "ATTACHMENT", "flow.png", "2", "", "current", "20", "1", "", ""],
                vec!["51", "ATTACHMENT", "flow.png", "1", "50", "current", "20", "1", "", ""],
                vec!["52", "ATTACHMENT", "gone.pdf", "1", "", "current", "20", "1", "", ""],
                vec!["70", "COMMENT", "", "1", "", "current", "31", "", "", ""],
            ],
        );
        let bodies = table(
            &["bodycontentid", "body", "contentid", "bodytypeid"],
            &[
                vec!["200", "<p>Welcome</p>", "2", "2"],
                vec!["201", deploy, "20", "2"],
                vec!["210", "<p>Old</p>", "21", "2"],
                vec!["300", "<p>Install it.</p>", "30", "2"],
                vec!["301", "<p>Ask <ac:link><ri:user ri:account-id=\"u1\" /></ac:link> or <ac:link><ri:user ri:account-id=\"u2\" /></ac:link>.</p>", "31", "2"],
            ],
        );
        let long = "long".repeat(20);
        let labels = table(&["labelid", "name", "owner", "namespace"], &[vec!["60", "runbook", "", "global"], vec!["61", "favourite", "u1", "my"], vec!["62", &long, "", "global"]]);
        let labellings = table(&["id", "labelid", "contentid"], &[vec!["600", "60", "20"], vec!["610", "61", "20"], vec!["620", "62", "30"]]);
        let spaces = table(&["spaceid", "spacename", "spacekey"], &[vec!["1", "Knowledge Base", "KB"]]);
        let users = table(&["user_key", "username"], &[vec!["u1", "u1"]]);
        zip_of(&[
            ("exportDescriptor.properties.gz", b"exportFormat=csv\nexportType=space\n"),
            ("spaces.csv.gz", &spaces),
            ("content.csv.gz", &gzipped(&content)),
            ("bodycontent.csv.gz", &bodies),
            ("label.csv.gz", &labels),
            ("content_label.csv.gz", &labellings),
            ("user_mapping.csv.gz", &users),
            ("attachments/20/50/1", b"old flow"),
            ("attachments/20/50/2", b"the flow"),
        ])
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
        // Grace is a Member with the e-mail address Confluence names her by, in another case; Ada is none.
        the_knowledge_base_comes_across(knowledge_base(), "Ask [@Grace](turtle:member/member-grace) or @ada.\n");
    }

    #[test]
    fn a_space_exported_as_csv_comes_across_as_the_same_space_exported_as_xml() {
        // Confluence Cloud's CSV export holds no names of people, only their account IDs.
        the_knowledge_base_comes_across(knowledge_base_csv(), "Ask @someone in Confluence or @someone in Confluence.\n");
    }

    /// Imports the space `knowledge_base` describes, in either form, and
    /// finds what it holds; the mention on "Contact" reads as `contact`.
    fn the_knowledge_base_comes_across(content: String, contact: &str) {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Wiki");

        let imported = import_confluence(&core, &grace, ToImportFromConfluence { area_id: area_id.clone(), parent_id: None, content })
            .unwrap()
            .output;

        assert_eq!(outline(&core, &grace, &area_id), ["Home", "  Guides (folder)", "    Set up", "    Deploy", "  Contact", "Import report: Knowledge Base (2023-11-14)"]);
        let home = &imported.entries[0];
        let (set_up, deploy) = (&home.children[0].children[0].id, &home.children[0].children[1].id);
        let flow = &list_attachments(&core, &grace, PageId { id: deploy.clone() }).unwrap().output.attachments;
        assert_eq!(flow.iter().map(|attachment| attachment.name.as_str()).collect::<Vec<_>>(), ["flow.png"]);
        assert_eq!(
            markdown_of(&core, &grace, deploy),
            format!("# Deploy\n\nFirst read [Set up](turtle:{set_up}) in Guides.\n\n![](<{}>)\n\n:confluence.toc[toc]\n\n```\na]]></b\n```\n", flow[0].url)
        );
        assert_eq!(markdown_of(&core, &grace, &home.children[1].id), contact);
        assert_eq!(get_page(&core, &grace, PageId { id: deploy.clone() }).unwrap().output.tags, ["runbook"]);
        let problems: Vec<_> = imported.problems.iter().map(|problem| (problem.file.as_str(), problem.message.as_str())).collect();
        assert_eq!(problems[0], ("Deploy/gone.pdf", "Its file is not in the export, so it was left out."));
        assert_eq!(problems[1].0, "Set up");
        assert!(problems[1].1.starts_with("Its Tag \"longlong"), "{}", problems[1].1);
        assert_eq!(problems.len(), 2);
    }

    #[test]
    fn the_report_is_a_page_beside_the_import_listing_the_pages_to_check_and_stays_marked_once_the_import_is_undone() {
        use crate::ops::import::record::{list_imports, undo_import, ImportToUndo, ImportsOfArea};
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Wiki");

        let imported = import_confluence(&core, &grace, ToImportFromConfluence { area_id: area_id.clone(), parent_id: None, content: knowledge_base() })
            .unwrap()
            .output;

        let home = &imported.entries[0];
        let (deploy, contact) = (&home.children[0].children[1].id, &home.children[1].id);
        let report = markdown_of(&core, &grace, imported.report_page_id.as_deref().unwrap());
        assert!(report.starts_with("This Page tells how the Confluence space **Knowledge Base** came across when it was imported on 2023-11-14. To undo the import, choose **Imports** in the menu of this Area."), "{report}");
        let to_check: Vec<&str> = report.lines().filter(|line| line.starts_with("- [ ]")).collect();
        assert_eq!(
            to_check,
            [
                format!("- [ ] [Deploy](turtle:{deploy}) (in Home › Guides): Placeholders: toc macro. Approximately: link to a folder."),
                format!("- [ ] [Contact](turtle:{contact}) (in Home): Approximately: mention."),
            ]
        );
        assert!(report.contains("- Deploy/gone.pdf: Its file is not in the export, so it was left out."), "{report}");
        let listed = list_imports(&core, &grace, ImportsOfArea { area_id: area_id.clone() }).unwrap().output.imports;
        assert_eq!(listed[0].source, "the Confluence space Knowledge Base");
        let report_page_id = imported.report_page_id.unwrap();
        assert_eq!(listed[0].report_page_id.as_deref(), Some(report_page_id.as_str()));

        undo_import(&core, &grace, ImportToUndo { import_id: imported.import_id }).unwrap();

        assert_eq!(outline(&core, &grace, &area_id), ["Import report: Knowledge Base (2023-11-14)"]);
        assert!(markdown_of(&core, &grace, &report_page_id).starts_with("> [!WARNING]\n> This import was undone on 2023-11-14."));
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
                ("mention of a Member or Guest", coverage::Outcome::Exact, 1),
            ]
        );
        assert_eq!(coverage.elements[0].pages, ["Deploy"]);
        // 9 of 12 elements exactly, 11 of 12 exactly or approximately.
        assert_eq!((coverage.exact_percent, coverage.kept_percent), (75, 91));
        let not_imported: Vec<_> = coverage.not_imported.iter().map(|item| (item.what.as_str(), item.count)).collect();
        assert_eq!(
            not_imported,
            [("Earlier versions of pages", 1), ("Drafts", 1), ("Pages in Confluence's trash", 1), ("Earlier versions of attachments", 1), ("Personal labels", 1)]
        );
    }

    #[test]
    fn the_report_on_a_csv_export_tells_what_it_holds_that_is_not_imported() {
        let core = fresh();
        let grace = member(&core, "Grace", false);

        let coverage = check_confluence_import(&core, &grace, ConfluenceExport { content: knowledge_base_csv() }).unwrap().output;

        assert_eq!((coverage.space.as_str(), coverage.pages, coverage.folders, coverage.attachments), ("Knowledge Base", 4, 1, 1));
        let not_imported: Vec<_> = coverage.not_imported.iter().map(|item| (item.what.as_str(), item.count)).collect();
        assert_eq!(
            not_imported,
            [
                ("Comments", 1),
                ("Earlier versions of pages", 1),
                ("Drafts", 1),
                ("Pages in Confluence's trash", 1),
                ("Earlier versions of attachments", 1),
                ("Personal labels", 1)
            ]
        );
    }

    #[test]
    fn a_page_a_csv_export_holds_no_content_for_comes_in_as_an_empty_page_not_a_folder() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Wiki");
        let columns = ["contentid", "contenttype", "title", "content_status", "spaceid"];
        let content = zip_of(&[
            ("spaces.csv.gz", &table(&["spaceid", "spacename", "spacekey"], &[vec!["1", "KB", "KB"]])),
            ("content.csv.gz", &table(&columns, &[vec!["2", "PAGE", "Empty", "current", "1"]])),
            ("bodycontent.csv.gz", &table(&["body", "contentid", "bodytypeid"], &[])),
        ]);

        let imported = import_confluence(&core, &grace, ToImportFromConfluence { area_id: area_id.clone(), parent_id: None, content }).unwrap().output;

        assert_eq!(outline(&core, &grace, &area_id), ["Empty", "Import report: KB (2023-11-14)"]);
        assert!(imported.problems.is_empty(), "{:?}", imported.problems);
    }

    #[test]
    fn a_link_by_title_goes_to_the_page_where_a_folder_shares_it_and_content_in_the_storage_format_wins() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Wiki");
        let columns = ["contentid", "contenttype", "title", "content_status", "spaceid", "child_position"];
        let link = "<p>See <ac:link><ri:page ri:content-title=\"Notes\" /></ac:link>.</p>";
        let content = zip_of(&[
            ("spaces.csv.gz", &table(&["spaceid", "spacename", "spacekey"], &[vec!["1", "KB", "KB"]])),
            (
                "content.csv.gz",
                &table(&columns, &[vec!["2", "PAGE", "Home", "current", "1", "1"], vec!["3", "PAGE", "Notes", "current", "1", "2"], vec!["4", "FOLDER", "Notes", "current", "1", "3"]]),
            ),
            ("bodycontent.csv.gz", &table(&["body", "contentid", "bodytypeid"], &[vec!["h1. Wiki markup", "2", "0"], vec![link, "2", "2"], vec!["", "3", "2"]])),
        ]);

        let imported = import_confluence(&core, &grace, ToImportFromConfluence { area_id: area_id.clone(), parent_id: None, content }).unwrap().output;

        let (home, notes) = (&imported.entries[0].id, &imported.entries[1].id);
        assert_eq!(markdown_of(&core, &grace, home), format!("See [Notes](turtle:{notes}).\n"));
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
    fn the_report_names_up_front_each_attachment_larger_than_this_turtle_accepts_and_the_import_leaves_it_out() {
        let mut core = fresh();
        // "the flow", the current version of Deploy's image, is 8 bytes.
        core.core.set_attachment_limit(5);
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Wiki");

        let coverage = check_confluence_import(&core, &grace, ConfluenceExport { content: knowledge_base() }).unwrap().output;
        let imported = import_confluence(&core, &grace, ToImportFromConfluence { area_id, parent_id: None, content: knowledge_base() })
            .unwrap()
            .output;

        assert_eq!(coverage.too_large, [TooLarge { file: "Deploy/flow.png".into(), bytes: 8 }]);
        assert_eq!(coverage.attachment_limit_bytes, 5);
        assert_eq!(coverage.attachments, 0);
        let deploy = &imported.entries[0].children[0].children[1].id;
        assert!(list_attachments(&core, &grace, PageId { id: deploy.clone() }).unwrap().output.attachments.is_empty());
        let problem = imported.problems.iter().find(|problem| problem.file == "Deploy/flow.png").unwrap();
        assert_eq!(
            problem.message,
            "It is 8 bytes, larger than 5 bytes, the most this Turtle accepts for one file, so it was left out. Whoever runs Turtle can raise that limit with TURTLE_MAX_ATTACHMENT_MB."
        );
    }

    #[test]
    fn a_started_import_from_confluence_tells_the_report_on_the_space_and_what_it_created() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace, "Wiki");

        let started = start_import_confluence(&core, &grace, ToImportFromConfluence { area_id: area_id.clone(), parent_id: None, content: knowledge_base() })
            .unwrap()
            .output;
        let progress =
            crate::ops::import::get_import_progress(&core, &grace, running::OfImport { import_id: started.import_id }).unwrap().output;

        assert_eq!(progress.stage, running::Stage::Done);
        assert_eq!(progress.coverage.unwrap().space, "Knowledge Base");
        // Four Pages of the space hold content to write.
        assert_eq!((progress.done, progress.total), (4, 4));
        assert_eq!(progress.imported.unwrap().entries[0].title, "Home");
        assert_eq!(outline(&core, &grace, &area_id), ["Home", "  Guides (folder)", "    Set up", "    Deploy", "  Contact", "Import report: Knowledge Base (2023-11-14)"]);
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
    fn a_zip_that_is_no_export_or_cannot_be_read_is_refused_with_what_to_do() {
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
            message(&[("Runbook.md", b"# Hi")]),
            "This ZIP is not a Confluence space export. In Confluence, export the space as CSV or XML and choose that ZIP."
        );
        assert_eq!(
            message(&[("spaces.csv.gz", b"spaceid,spacename,spacekey\n1,KB,KB\n"), ("content.csv.gz", b"\x1f\x8b broken")]),
            "The export's content.csv could not be read. Export the space from Confluence again and choose the new ZIP."
        );
        assert_eq!(
            message(&[("entities.xml", b"<hibernate-generic><object class=\"Page\">")]),
            "The export's entities.xml could not be read. Export the space from Confluence again and choose the new ZIP."
        );
    }
}
