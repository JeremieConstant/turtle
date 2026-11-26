//! Reading a space from Confluence's XML export: `entities.xml`, every
//! object of the space as Hibernate writes it (`entities`), beside the
//! attachments. Confluence Data Center exports this; Confluence Cloud does
//! until 1 December 2026.

use super::entities::{self, Object};
use super::{Archive, Attachment, Labelling, Page, Space, Unimported, STORAGE_FORMAT};
use crate::ops::tree::Kind;
use crate::ops::OpError;
use std::collections::{HashMap, HashSet};
use std::io::Read;

/// The objects of `entities.xml` the import reads; every other class is
/// only counted.
const READ_CLASSES: [&str; 7] = ["Space", "Page", "Attachment", "Label", "Labelling", "ConfluenceUserImpl", "Comment"];

/// Reads the space whose `entities.xml` lies at `prefix` in the ZIP.
pub(super) fn read(archive: &mut Archive, prefix: &str) -> Result<Space, OpError> {
    let xml = {
        let mut file = archive.by_name(&format!("{prefix}entities.xml")).map_err(|_| unreadable())?;
        let mut xml = String::new();
        file.read_to_string(&mut xml).map_err(|_| unreadable())?;
        xml
    };
    let (objects, counts) = entities::read(&xml, |object| READ_CLASSES.contains(&object.class.as_str())).ok_or_else(unreadable)?;
    let (mut space, body_ids) = space_of(objects, &counts)?;
    // The content of each page's latest version, read in a second pass so
    // that the content of every earlier version is never held.
    let wanted: HashSet<&str> = body_ids.values().map(String::as_str).collect();
    let (bodies, _) =
        entities::read(&xml, |object| object.class == "BodyContent" && wanted.contains(object.id.as_str())).ok_or_else(unreadable)?;
    drop(xml);
    let bodies: HashMap<&str, &Object> = bodies.iter().map(|body| (body.id.as_str(), body)).collect();
    for page in &mut space.pages {
        let body = body_ids.get(&page.id).and_then(|id| bodies.get(id.as_str()));
        page.storage = body
            .filter(|body| body.text("bodyType").is_none_or(|kind| kind == STORAGE_FORMAT))
            // The export writes `]]>` inside a body's CDATA as `]] >`.
            .map(|body| body.text("body").unwrap_or_default().replace("]] >", "]]>"));
    }
    Ok(space)
}

fn unreadable() -> OpError {
    OpError::InvalidInput("The export's entities.xml could not be read. Export the space from Confluence again and choose the new ZIP.".into())
}

/// Whether an object is the current version of what it is: not an earlier
/// one, a draft or in the trash.
fn is_current(object: &Object) -> bool {
    object.text("contentStatus").is_none_or(|status| status == "current") && !is_earlier_version(object)
}

fn is_earlier_version(object: &Object) -> bool {
    object.points_to("originalVersion").is_some() || object.text("originalVersionId").is_some_and(|id| !id.is_empty())
}

/// The space the objects hold, with the ID of each page's content by the
/// page's ID. The export has no folders of its own: a page without content
/// is taken for one.
fn space_of(objects: Vec<Object>, counts: &HashMap<String, u32>) -> Result<(Space, HashMap<String, String>), OpError> {
    let mut by_class: HashMap<&str, Vec<&Object>> = HashMap::new();
    for object in &objects {
        by_class.entry(object.class.as_str()).or_default().push(object);
    }
    let of = |class: &str| by_class.get(class).cloned().unwrap_or_default();
    let space = *super::the_one_space(&of("Space"))?;
    let in_space = |object: &Object| object.points_to("space").is_none_or(|id| id == space.id);
    let all_pages = of("Page");
    let mut body_ids = HashMap::new();
    let pages: Vec<Page> = all_pages
        .iter()
        .filter(|page| is_current(page) && in_space(page))
        .map(|page| {
            let body_id = page.collections.get("bodyContents").and_then(|ids| ids.first()).cloned();
            let kind = if body_id.is_some() { Kind::Page } else { Kind::Folder };
            if let Some(body_id) = body_id {
                body_ids.insert(page.id.clone(), body_id);
            }
            Page {
                id: page.id.clone(),
                title: page.text("title").unwrap_or_default().to_string(),
                parent: page.points_to("parent").map(str::to_string),
                position: page.text("position").and_then(|position| position.parse().ok()),
                kind,
                storage: None,
            }
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
    let labelled = of("Labelling").into_iter().filter_map(|labelling| {
        let (name, namespace) = labelling.points_to("label").and_then(|id| labels.get(id))?;
        Some(Labelling { page_id: labelling.points_to("content")?, name, namespace })
    });
    let labels = super::Labels::of(labelled, &page_ids);
    let people = of("ConfluenceUserImpl")
        .iter()
        .filter_map(|person| Some((person.id.clone(), person.text("name")?.to_string())))
        .collect();
    // Confluence Data Center may keep a person's e-mail address apart from
    // their name; Confluence Cloud names them by it.
    let emails = of("ConfluenceUserImpl")
        .iter()
        .filter_map(|person| Some((person.id.clone(), person.text("email").filter(|email| !email.is_empty())?.to_string())))
        .collect();
    let status_count = |objects: &[&Object], status: &str| {
        objects.iter().filter(|object| in_space(object) && object.text("contentStatus") == Some(status) && !is_earlier_version(object)).count() as u32
    };
    let counted = |class: &str| counts.get(class).copied().unwrap_or(0);
    let not_imported = Unimported {
        comments: of("Comment").iter().filter(|comment| is_current(comment)).count() as u32,
        earlier_versions_of_pages: all_pages.iter().filter(|page| is_earlier_version(page)).count() as u32,
        drafts: status_count(&all_pages, "draft"),
        archived_pages: status_count(&all_pages, "archived"),
        pages_in_trash: status_count(&all_pages, "deleted"),
        earlier_versions_of_attachments: all_attachments.iter().filter(|attachment| is_earlier_version(attachment)).count() as u32,
        personal_labels: labels.personal,
        other_labels: labels.other,
        space_permissions: counted("SpacePermission"),
        page_restrictions: counted("ContentPermissionSet"),
        blog_posts: counted("BlogPost"),
        ..Unimported::default()
    };
    let space = Space {
        name: space.text("name").unwrap_or_default().to_string(),
        key: space.text("key").unwrap_or_default().to_string(),
        pages,
        attachments,
        tags: labels.tags,
        people,
        emails,
        not_imported,
    };
    Ok((space, body_ids))
}
