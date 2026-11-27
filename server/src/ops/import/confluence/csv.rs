//! Reading a space from Confluence's CSV export: one table of the space's
//! database per file, such as `content.csv.gz` for its pages, folders and
//! attachments, beside the attachments. Confluence Cloud exports this.
//! A table may be gzipped as its name says, or already unpacked.

use super::{Archive, Attachment, Labelling, Labels, Page, Space, Unimported, STORAGE_FORMAT};
use crate::ops::tree::Kind;
use crate::ops::OpError;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};

/// Reads the space whose tables lie at `prefix` in the ZIP.
pub(super) fn read(archive: &mut Archive, prefix: &str) -> Result<Space, OpError> {
    let spaces = Table::read(archive, prefix, "spaces", Need::Required, |_| true)?;
    let space = super::the_one_space(&spaces.rows)?;
    let space_id = spaces.get(space, "spaceid").to_string();
    let content = Table::read(archive, prefix, "content", Need::Required, |_| true)?;
    let all: Vec<Content> = content.rows.iter().map(|record| Content::of(&content, record)).collect();
    let in_space = |item: &Content| item.space_id.is_empty() || item.space_id == space_id;
    let current = |item: &Content| item.status == "current" && item.earlier_of.is_empty();

    let mut pages: Vec<Page> = all
        .iter()
        .filter(|item| matches!(item.kind, "PAGE" | "FOLDER") && current(item) && in_space(item))
        .map(|item| Page {
            id: item.id.to_string(),
            title: item.title.to_string(),
            parent: Some(item.parent.to_string()).filter(|parent| !parent.is_empty()),
            position: item.position.parse().ok(),
            kind: if item.kind == "FOLDER" { Kind::Folder } else { Kind::Page },
            storage: None,
        })
        .collect();
    let with_content: HashSet<&str> = pages.iter().filter(|page| page.kind == Kind::Page).map(|page| page.id.as_str()).collect();
    let bodies = Table::read(archive, prefix, "bodycontent", Need::Required, |row| with_content.contains(row.get("contentid")))?;
    // A page's content in the storage format, or nothing where the export
    // holds it only in another format; the storage format wins.
    let mut storage: HashMap<String, Option<String>> = HashMap::new();
    for body in &bodies.rows {
        let page_id = bodies.get(body, "contentid").to_string();
        if bodies.get(body, "bodytypeid") == STORAGE_FORMAT {
            storage.insert(page_id, Some(bodies.get(body, "body").to_string()));
        } else {
            storage.entry(page_id).or_insert(None);
        }
    }
    for page in pages.iter_mut().filter(|page| page.kind == Kind::Page) {
        // A page the export holds no content for is empty.
        page.storage = storage.remove(&page.id).unwrap_or_else(|| Some(String::new()));
    }

    let imported: HashSet<&str> = pages.iter().map(|page| page.id.as_str()).collect();
    let attachments = all
        .iter()
        .filter(|item| item.kind == "ATTACHMENT" && current(item) && imported.contains(item.page))
        .map(|item| Attachment {
            id: item.id.to_string(),
            page_id: item.page.to_string(),
            name: item.title.to_string(),
            version: if item.version.is_empty() { "1".to_string() } else { item.version.to_string() },
        })
        .collect();

    let label_table = Table::read(archive, prefix, "label", Need::Optional, |_| true)?;
    let names: HashMap<&str, (&str, &str)> = label_table
        .rows
        .iter()
        .map(|label| (label_table.get(label, "labelid"), (label_table.get(label, "name"), label_table.get(label, "namespace"))))
        .collect();
    let labellings = Table::read(archive, prefix, "content_label", Need::Optional, |_| true)?;
    let labelled = labellings.rows.iter().filter_map(|labelling| {
        let (name, namespace) = names.get(labellings.get(labelling, "labelid"))?;
        Some(Labelling { page_id: labellings.get(labelling, "contentid"), name, namespace })
    });
    let labels = Labels::of(labelled, &imported);

    // Confluence Cloud writes no names here, only the person's account ID
    // twice; a name is kept only where there is one.
    let users = Table::read(archive, prefix, "user_mapping", Need::Optional, |_| true)?;
    let people = users
        .rows
        .iter()
        .map(|user| (users.get(user, "user_key"), users.get(user, "username")))
        .filter(|(key, name)| !name.is_empty() && name != key)
        .map(|(key, name)| (key.to_string(), name.to_string()))
        .collect();

    let count = |keep: &dyn Fn(&Content) -> bool| all.iter().filter(|item| in_space(item) && keep(item)).count() as u32;
    let rows_of = |archive: &mut Archive, name: &str| -> Result<u32, OpError> {
        Ok(Table::read(archive, prefix, name, Need::Optional, |_| true)?.rows.len() as u32)
    };
    let not_imported = Unimported {
        comments: count(&|item| item.kind == "COMMENT" && current(item)),
        earlier_versions_of_pages: count(&|item| item.kind == "PAGE" && !item.earlier_of.is_empty() && item.status == "current"),
        // A draft of a page is held as a version of it.
        drafts: count(&|item| item.kind == "PAGE" && item.status == "draft"),
        archived_pages: count(&|item| item.kind == "PAGE" && item.status == "archived" && item.earlier_of.is_empty()),
        pages_in_trash: count(&|item| item.kind == "PAGE" && item.status == "deleted" && item.earlier_of.is_empty()),
        earlier_versions_of_attachments: count(&|item| item.kind == "ATTACHMENT" && !item.earlier_of.is_empty()),
        personal_labels: labels.personal,
        other_labels: labels.other,
        space_permissions: rows_of(archive, "spacepermissions")?,
        page_restrictions: rows_of(archive, "content_perm_set")?,
        blog_posts: count(&|item| item.kind == "BLOGPOST" && current(item)),
        whiteboards: count(&|item| item.kind == "WHITEBOARD" && item.earlier_of.is_empty() && item.status != "deleted"),
        databases: count(&|item| item.kind == "DATABASE" && item.earlier_of.is_empty() && item.status != "deleted"),
    };
    Ok(Space {
        name: spaces.get(space, "spacename").to_string(),
        key: spaces.get(space, "spacekey").to_string(),
        pages,
        attachments,
        tags: labels.tags,
        people,
        emails: HashMap::new(),
        not_imported,
    })
}

/// A row of the `content` table: a page, folder, attachment, comment or
/// other piece of the space, in one of its versions.
struct Content<'t> {
    id: &'t str,
    kind: &'t str,
    title: &'t str,
    version: &'t str,
    /// `current`, `draft`, `archived` or `deleted`.
    status: &'t str,
    /// For an earlier version, the ID of the latest; empty for the latest.
    earlier_of: &'t str,
    space_id: &'t str,
    parent: &'t str,
    position: &'t str,
    /// An attachment's page.
    page: &'t str,
}

impl<'t> Content<'t> {
    fn of(table: &'t Table, row: &'t csv::StringRecord) -> Content<'t> {
        let get = |column| table.get(row, column);
        Content {
            id: get("contentid"),
            kind: get("contenttype"),
            title: get("title"),
            version: get("version"),
            status: get("content_status"),
            earlier_of: get("prevver"),
            space_id: get("spaceid"),
            parent: get("parentid"),
            position: get("child_position"),
            page: get("pageid"),
        }
    }
}

/// Whether an export without a table is unreadable.
enum Need {
    Required,
    Optional,
}

/// A table of the export, with the rows kept.
struct Table {
    columns: HashMap<String, usize>,
    rows: Vec<csv::StringRecord>,
}

/// A row being read, before it is kept.
struct Row<'a> {
    columns: &'a HashMap<String, usize>,
    record: &'a csv::StringRecord,
}

impl Row<'_> {
    fn get(&self, column: &str) -> &str {
        cell(self.columns, self.record, column)
    }
}

/// What a row holds in a column; empty where the table lacks the column.
fn cell<'r>(columns: &HashMap<String, usize>, record: &'r csv::StringRecord, column: &str) -> &'r str {
    columns.get(column).and_then(|index| record.get(*index)).unwrap_or_default()
}

impl Table {
    /// Reads the table `name`, keeping the rows `keep` keeps. A table the
    /// export lacks is empty when it is optional.
    fn read(archive: &mut Archive, prefix: &str, name: &str, need: Need, keep: impl Fn(&Row) -> bool) -> Result<Table, OpError> {
        let unreadable = || {
            OpError::InvalidInput(format!(
                "The export's {name}.csv could not be read. Export the space from Confluence again and choose the new ZIP."
            ))
        };
        let file = [format!("{prefix}{name}.csv.gz"), format!("{prefix}{name}.csv")].into_iter().find(|file| archive.index_for_name(file).is_some());
        let Some(file) = file else {
            return match need {
                Need::Required => Err(unreadable()),
                Need::Optional => Ok(Table { columns: HashMap::new(), rows: Vec::new() }),
            };
        };
        // Read as it is unpacked, row by row: a table such as the pages'
        // content, with every earlier version, is far larger than what is
        // kept of it.
        let mut file = BufReader::new(archive.by_name(&file).map_err(|_| unreadable())?);
        // Gzipped, as the name says, or already unpacked, as the export
        // often holds it despite the name.
        let gzipped = file.fill_buf().map_err(|_| unreadable())?.starts_with(&[0x1f, 0x8b]);
        let unpacked: Box<dyn Read + '_> = if gzipped { Box::new(flate2::bufread::MultiGzDecoder::new(file)) } else { Box::new(file) };
        let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(unpacked);
        let columns: HashMap<String, usize> =
            reader.headers().map_err(|_| unreadable())?.iter().enumerate().map(|(index, column)| (column.to_string(), index)).collect();
        let mut rows = Vec::new();
        for record in reader.records() {
            let record = record.map_err(|_| unreadable())?;
            if keep(&Row { columns: &columns, record: &record }) {
                rows.push(record);
            }
        }
        Ok(Table { columns, rows })
    }

    fn get<'r>(&self, row: &'r csv::StringRecord, column: &str) -> &'r str {
        cell(&self.columns, row, column)
    }
}
