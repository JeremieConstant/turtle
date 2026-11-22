//! Attachments: images and files that belong to a Page. Whoever may read the
//! Page may read its Attachments; adding one needs the Write Role.
//!
//! The files themselves live in the data directory, each under the SHA-256
//! of its content, so the same file attached twice is stored once.

use super::accounts::{hex, random_hex};
use super::tree::{self, PageId};
use super::{Catalogue, Core, OpError, OpResult, Principal};
use anyhow::Context;
use base64::Engine;
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// How large a file may be when whoever runs Turtle does not say.
pub const DEFAULT_ATTACHMENT_LIMIT: u64 = 25 * 1024 * 1024;

const NO_SUCH_ATTACHMENT: &str = "There is no such Attachment.";

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue.add(
        "attach_file",
        "Attaches an image or file to a Page and returns the Attachment. The content is sent in base64. An image is shown in the Page by putting its `url` in the content, in Markdown as ![description](url). Files larger than the limit Turtle was started with (25 MB unless set otherwise) are refused. Needs the Write Role.",
        attach_file,
    );
    catalogue
        .add(
            "list_attachments",
            "Lists the Attachments of a Page, the oldest first, with their names and sizes.",
            list_attachments,
        )
        .changes = false;
    catalogue
        .add(
            "read_attachment",
            "Returns an Attachment with its content in base64. In a browser, its `url` downloads it.",
            read_attachment,
        )
        .changes = false;
}

/// A file to attach.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewAttachment {
    pub page_id: String,
    /// The file's name, such as "diagram.png". Any folders before it are left out.
    pub name: String,
    /// The file's content, in base64.
    pub content: String,
}

/// An image or file that belongs to a Page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Attachment {
    pub id: String,
    pub page_id: String,
    pub name: String,
    /// In bytes.
    pub size: u64,
    /// What kind of file it is, such as "image/png", judged by its name.
    pub content_type: String,
    /// Whether it is an image a Page can show. Other files, SVG images
    /// among them since they can carry scripts, are only downloaded.
    pub shown_as_image: bool,
    /// Where it is downloaded from, and the address to show an image by.
    pub url: String,
    /// Seconds since the Unix epoch.
    pub attached_at: i64,
}

/// The Attachments of a Page.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Attachments {
    /// The oldest first.
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AttachmentId {
    pub id: String,
}

/// An Attachment with what it holds.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AttachmentContent {
    pub attachment: Attachment,
    /// The file's content, in base64.
    pub content: String,
}

pub fn attach_file(core: &Core, principal: &Principal, input: NewAttachment) -> OpResult<Attachment> {
    // Access is checked before the input, so an unreadable Page stays hidden
    // whatever is sent.
    tree::page_to_change(core, principal, &input.page_id)?;
    let name = valid_name(&input.name)?;
    let content = base64::engine::general_purpose::STANDARD
        .decode(input.content.trim())
        .map_err(|_| OpError::InvalidInput("Send the file's content in base64.".into()))?;
    let limit = core.attachment_limit();
    if content.len() as u64 > limit {
        return Err(OpError::InvalidInput(format!(
            "This file is larger than {}, the most this Turtle accepts. Choose a smaller file, or ask whoever runs Turtle to raise the limit.",
            size_in_words(limit)
        )));
    }
    // The file is written before the database is taken, so a large upload
    // holds up nobody else.
    let content_hash = store_file(core.attachments_dir(), &content)?;
    let attachment = Attachment {
        id: random_hex(16)?,
        url: String::new(),
        page_id: input.page_id,
        content_type: content_type_of(&name).into(),
        shown_as_image: false,
        name,
        size: content.len() as u64,
        attached_at: core.now(),
    };
    let db = core.db();
    // Removing a Page for good may have taken the file away meanwhile, if no
    // other Attachment had it. That removal holds the database while it looks
    // and removes, so stored again here, under the same lock, the file stays
    // until this Attachment refers to it.
    store_file(core.attachments_dir(), &content)?;
    db.execute(
        "INSERT INTO attachments (id, page_id, name, size, content_type, content_hash, attached_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            attachment.id,
            attachment.page_id,
            attachment.name,
            attachment.size as i64,
            attachment.content_type,
            content_hash,
            attachment.attached_at
        ],
    )?;
    Ok(with_url(attachment).into())
}

pub fn list_attachments(core: &Core, principal: &Principal, input: PageId) -> OpResult<Attachments> {
    tree::get_page(core, principal, PageId { id: input.id.clone() })?;
    let db = core.db();
    let mut statement = db.prepare(&format!("{SELECT} WHERE page_id = ?1 ORDER BY attached_at, rowid"))?;
    let attachments = statement
        .query_map([&input.id], |row| attachment_from(row).map(|(attachment, _)| attachment))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Attachments { attachments }.into())
}

pub fn read_attachment(core: &Core, principal: &Principal, input: AttachmentId) -> OpResult<AttachmentContent> {
    let (attachment, content) = open_attachment(core, principal, &input.id)?;
    Ok(AttachmentContent {
        attachment,
        content: base64::engine::general_purpose::STANDARD.encode(content),
    }
    .into())
}

/// An Attachment the caller may read, with its content, for adapters that
/// hand out the file itself.
pub fn open_attachment(core: &Core, principal: &Principal, id: &str) -> Result<(Attachment, Vec<u8>), OpError> {
    super::areas::signed_in(principal)?;
    let found = find(&core.db(), id)?;
    let Some((attachment, content_hash)) = found else {
        return Err(OpError::NotFound(NO_SUCH_ATTACHMENT.into()));
    };
    // Hidden is not found: an Attachment of a Page the caller cannot read
    // answers like one that does not exist.
    match tree::get_page(core, principal, PageId { id: attachment.page_id.clone() }) {
        Ok(_) => {}
        Err(OpError::NotFound(_)) => return Err(OpError::NotFound(NO_SUCH_ATTACHMENT.into())),
        Err(other) => return Err(other),
    }
    let path = core.attachments_dir().join(&content_hash);
    let content = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    Ok((attachment, content))
}

/// The files the Attachments of these Pages hold, to give to
/// `remove_unattached_files` once the Pages are gone.
pub(super) fn files_of(db: &Connection, pages: &[String]) -> rusqlite::Result<Vec<String>> {
    let mut statement = db.prepare("SELECT DISTINCT content_hash FROM attachments WHERE page_id = ?1")?;
    let mut hashes = Vec::new();
    for page in pages {
        for hash in statement.query_map([page], |row| row.get::<_, String>(0))? {
            let hash = hash?;
            if !hashes.contains(&hash) {
                hashes.push(hash);
            }
        }
    }
    Ok(hashes)
}

/// Removes those of these files that no Attachment holds any more: another
/// Attachment with the same content keeps its file. Called with the database
/// held, which `attach_file` also takes before it relies on a file being there.
pub(super) fn remove_unattached_files(core: &Core, db: &Connection, hashes: &[String]) -> rusqlite::Result<()> {
    let mut still_attached = db.prepare("SELECT EXISTS (SELECT 1 FROM attachments WHERE content_hash = ?1)")?;
    for hash in hashes {
        if still_attached.query_row([hash], |row| row.get::<_, bool>(0))? {
            continue;
        }
        let path = core.attachments_dir().join(hash);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            // What it belonged to is gone already; a file left behind only
            // takes up space, so this is no reason to fail the removal.
            Err(error) => eprintln!("removing {} failed: {error}", path.display()),
        }
    }
    Ok(())
}

const SELECT: &str = "SELECT id, page_id, name, size, content_type, attached_at, content_hash FROM attachments";

fn find(db: &Connection, id: &str) -> rusqlite::Result<Option<(Attachment, String)>> {
    db.query_row(&format!("{SELECT} WHERE id = ?1"), [id], attachment_from).optional()
}

fn attachment_from(row: &rusqlite::Row) -> rusqlite::Result<(Attachment, String)> {
    let attachment = Attachment {
        id: row.get(0)?,
        page_id: row.get(1)?,
        name: row.get(2)?,
        size: row.get::<_, i64>(3)? as u64,
        content_type: row.get(4)?,
        attached_at: row.get(5)?,
        url: String::new(),
        shown_as_image: false,
    };
    Ok((with_url(attachment), row.get(6)?))
}

/// Fills in what follows from the rest: the address and whether it is shown as an image.
fn with_url(attachment: Attachment) -> Attachment {
    Attachment {
        url: format!("/api/attachments/{}", attachment.id),
        shown_as_image: IMAGE_TYPES.iter().any(|(_, content_type)| *content_type == attachment.content_type),
        ..attachment
    }
}

/// Writes the content under its hash, unless it is there already, and
/// returns the hash. It is written beside its place first and then moved
/// there, so a file under a hash is always complete.
fn store_file(dir: &Path, content: &[u8]) -> Result<String, OpError> {
    let hash = hex(&Sha256::digest(content));
    let path = dir.join(&hash);
    if path.exists() {
        return Ok(hash);
    }
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let partial: PathBuf = dir.join(format!(".{hash}.{}", random_hex(8)?));
    std::fs::write(&partial, content).with_context(|| format!("writing {}", partial.display()))?;
    std::fs::rename(&partial, &path).with_context(|| format!("moving {} into place", partial.display()))?;
    Ok(hash)
}

/// The name without any folders a browser or Agent put before it.
fn valid_name(name: &str) -> Result<String, OpError> {
    let name = name.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    if name.is_empty() {
        return Err(OpError::InvalidInput("Give the file a name.".into()));
    }
    if name.chars().count() > 255 {
        return Err(OpError::InvalidInput("Give the file a name of at most 255 characters.".into()));
    }
    Ok(name.chars().filter(|c| !c.is_control()).collect())
}

/// Only these are shown as images: they cannot carry scripts, unlike SVG.
const IMAGE_TYPES: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
];

fn content_type_of(name: &str) -> &'static str {
    let extension = name.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase());
    let known = IMAGE_TYPES.iter().chain(&[("pdf", "application/pdf"), ("txt", "text/plain")]);
    known
        .into_iter()
        .find(|(known, _)| extension.as_deref() == Some(*known))
        .map_or("application/octet-stream", |(_, content_type)| content_type)
}

/// "25 MB" for 25 MiB, as people read file sizes.
fn size_in_words(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    if bytes >= MB && bytes.is_multiple_of(MB) {
        format!("{} MB", bytes / MB)
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::super::permissions::Role;
    use super::super::testing::{fresh, give_role, member};
    use super::super::{areas, trash, tree};
    use super::*;

    struct Setup {
        core: super::super::testing::TestCore,
        grace: Principal,
        page_id: String,
        area_id: String,
    }

    /// Grace's Area with one Page in it.
    fn setup() -> Setup {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = areas::create_area(&core, &grace, areas::NewArea { name: "Engineering".into() })
            .unwrap()
            .output
            .id;
        let page_id = tree::create_page(
            &core,
            &grace,
            tree::NewEntry { area_id: area_id.clone(), parent_id: None, title: "Runbook".into() }.into(),
        )
        .unwrap()
        .output
        .id;
        Setup { core, grace, page_id, area_id }
    }

    fn attach(setup: &Setup, by: &Principal, name: &str, content: &str) -> Result<Attachment, OpError> {
        attach_file(
            &setup.core,
            by,
            NewAttachment {
                page_id: setup.page_id.clone(),
                name: name.into(),
                content: content.into(),
            },
        )
        .map(|reply| reply.output)
    }

    // "hello" in base64.
    const HELLO: &str = "aGVsbG8=";

    #[test]
    fn a_writer_attaches_a_file_that_readers_list_and_download() {
        let setup = setup();
        let attached = attach(&setup, &setup.grace, "notes/deploy.txt", HELLO).unwrap();
        assert_eq!(attached.name, "deploy.txt");
        assert_eq!(attached.size, 5);
        assert_eq!(attached.content_type, "text/plain");
        assert_eq!(attached.url, format!("/api/attachments/{}", attached.id));

        let ken = member(&setup.core, "Ken", false);
        give_role(&setup.core, &setup.area_id, &ken, Role::Read);
        let listed = list_attachments(&setup.core, &ken, PageId { id: setup.page_id.clone() }).unwrap().output;
        assert_eq!(listed.attachments, vec![attached.clone()]);
        let read = read_attachment(&setup.core, &ken, AttachmentId { id: attached.id }).unwrap().output;
        assert_eq!(read.content, HELLO);
    }

    #[test]
    fn attaching_needs_the_write_role() {
        let setup = setup();
        let ken = member(&setup.core, "Ken", false);
        give_role(&setup.core, &setup.area_id, &ken, Role::Read);
        let refused = attach(&setup, &ken, "deploy.txt", HELLO).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
    }

    #[test]
    fn without_read_on_the_page_an_attachment_answers_like_one_that_does_not_exist() {
        let setup = setup();
        let attached = attach(&setup, &setup.grace, "deploy.txt", HELLO).unwrap();
        let linus = member(&setup.core, "Linus", false);
        let hidden = read_attachment(&setup.core, &linus, AttachmentId { id: attached.id }).unwrap_err();
        let missing = read_attachment(&setup.core, &linus, AttachmentId { id: "made-up".into() }).unwrap_err();
        assert_eq!((hidden.kind(), hidden.message()), (missing.kind(), missing.message()));
        assert_eq!(hidden.kind(), "not_found");
    }

    #[test]
    fn an_attachment_of_a_page_in_the_trash_is_out_of_reach() {
        let setup = setup();
        let attached = attach(&setup, &setup.grace, "deploy.txt", HELLO).unwrap();
        let page = super::super::trash::EntryId { id: setup.page_id.clone() };
        super::super::trash::move_to_trash(&setup.core, &setup.grace, page).unwrap();
        let hidden = read_attachment(&setup.core, &setup.grace, AttachmentId { id: attached.id }).unwrap_err();
        assert_eq!(hidden.message(), NO_SUCH_ATTACHMENT);
    }

    #[test]
    fn files_are_stored_in_the_data_directory_under_their_content_hash_once() {
        let setup = setup();
        attach(&setup, &setup.grace, "one.txt", HELLO).unwrap();
        attach(&setup, &setup.grace, "two.txt", HELLO).unwrap();
        // The SHA-256 of "hello".
        let hash = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let stored: Vec<_> = std::fs::read_dir(setup.core.attachments_dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(stored, vec![hash.to_string()]);
        assert_eq!(std::fs::read(setup.core.attachments_dir().join(hash)).unwrap(), b"hello");
    }

    // The SHA-256 of "hello" and of "bye".
    const HELLO_FILE: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    const BYE: &str = "Ynll";
    const BYE_FILE: &str = "b49f425a7e1f9cff3856329ada223f2f9d368f15a00cf48df16ca95986137fe8";

    fn stored_files(setup: &Setup) -> Vec<String> {
        let mut stored: Vec<_> = std::fs::read_dir(setup.core.attachments_dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        stored.sort();
        stored
    }

    fn another_page(setup: &Setup, title: &str) -> String {
        let entry = tree::NewEntry { area_id: setup.area_id.clone(), parent_id: None, title: title.into() };
        tree::create_page(&setup.core, &setup.grace, entry.into()).unwrap().output.id
    }

    fn trash_page(setup: &Setup, page_id: &str) {
        trash::move_to_trash(&setup.core, &setup.grace, trash::EntryId { id: page_id.into() }).unwrap();
    }

    fn delete_for_good(setup: &Setup, page_id: &str) {
        trash::delete_from_trash(&setup.core, &setup.grace, trash::EntryId { id: page_id.into() }).unwrap();
    }

    #[test]
    fn the_files_of_a_page_deleted_from_the_trash_are_removed_but_not_before() {
        let setup = setup();
        attach(&setup, &setup.grace, "deploy.txt", HELLO).unwrap();
        attach(&setup, &setup.grace, "farewell.txt", BYE).unwrap();
        trash_page(&setup, &setup.page_id);
        assert_eq!(stored_files(&setup), [HELLO_FILE, BYE_FILE], "a Page in the Trash can still be restored");

        delete_for_good(&setup, &setup.page_id);

        assert!(stored_files(&setup).is_empty());
    }

    #[test]
    fn a_file_another_attachment_holds_stays_until_that_one_is_removed_too() {
        let setup = setup();
        let other_page = another_page(&setup, "Checklist");
        attach(&setup, &setup.grace, "deploy.txt", HELLO).unwrap();
        let kept = attach_file(
            &setup.core,
            &setup.grace,
            NewAttachment { page_id: other_page.clone(), name: "copy.txt".into(), content: HELLO.into() },
        )
        .unwrap()
        .output;
        trash_page(&setup, &setup.page_id);
        delete_for_good(&setup, &setup.page_id);

        assert_eq!(stored_files(&setup), [HELLO_FILE]);
        let read = read_attachment(&setup.core, &setup.grace, AttachmentId { id: kept.id }).unwrap().output;
        assert_eq!(read.content, HELLO);

        trash_page(&setup, &other_page);
        delete_for_good(&setup, &other_page);
        assert!(stored_files(&setup).is_empty());
    }

    #[test]
    fn the_files_of_pages_beneath_a_folder_go_when_the_folder_is_deleted_from_the_trash() {
        let setup = setup();
        let folder = tree::create_folder(
            &setup.core,
            &setup.grace,
            tree::NewEntry { area_id: setup.area_id.clone(), parent_id: None, title: "Guides".into() },
        )
        .unwrap()
        .output
        .id;
        let beneath = tree::create_page(
            &setup.core,
            &setup.grace,
            tree::NewEntry { area_id: setup.area_id.clone(), parent_id: Some(folder.clone()), title: "Setup".into() }
                .into(),
        )
        .unwrap()
        .output
        .id;
        attach_file(
            &setup.core,
            &setup.grace,
            NewAttachment { page_id: beneath, name: "farewell.txt".into(), content: BYE.into() },
        )
        .unwrap();
        attach(&setup, &setup.grace, "deploy.txt", HELLO).unwrap();
        trash_page(&setup, &folder);

        delete_for_good(&setup, &folder);

        assert_eq!(stored_files(&setup), [HELLO_FILE], "the Page outside the Folder keeps its file");
    }

    #[test]
    fn the_files_of_what_expires_in_the_trash_are_removed_with_it() {
        let setup = setup();
        let other_area = areas::create_area(&setup.core, &setup.grace, areas::NewArea { name: "Sales".into() })
            .unwrap()
            .output
            .id;
        let in_other_area = tree::create_page(
            &setup.core,
            &setup.grace,
            tree::NewEntry { area_id: other_area.clone(), parent_id: None, title: "Pitch".into() }.into(),
        )
        .unwrap()
        .output
        .id;
        attach_file(
            &setup.core,
            &setup.grace,
            NewAttachment { page_id: in_other_area, name: "farewell.txt".into(), content: BYE.into() },
        )
        .unwrap();
        attach(&setup, &setup.grace, "deploy.txt", HELLO).unwrap();
        trash_page(&setup, &setup.page_id);
        trash::delete_area(&setup.core, &setup.grace, areas::AreaId { id: other_area }).unwrap();

        setup.core.advance(trash::KEEP_DAYS * 24 * 60 * 60 + 1);
        assert_eq!(trash::remove_expired(&setup.core).unwrap(), 2);

        assert!(stored_files(&setup).is_empty());
    }

    #[test]
    fn a_file_attached_again_after_its_last_attachment_went_is_stored_again() {
        let setup = setup();
        let other_page = another_page(&setup, "Checklist");
        attach(&setup, &setup.grace, "deploy.txt", HELLO).unwrap();
        trash_page(&setup, &setup.page_id);
        delete_for_good(&setup, &setup.page_id);

        let again = attach_file(
            &setup.core,
            &setup.grace,
            NewAttachment { page_id: other_page, name: "deploy.txt".into(), content: HELLO.into() },
        )
        .unwrap()
        .output;

        assert_eq!(stored_files(&setup), [HELLO_FILE]);
        let read = read_attachment(&setup.core, &setup.grace, AttachmentId { id: again.id }).unwrap().output;
        assert_eq!(read.content, HELLO);
    }

    #[test]
    fn a_file_larger_than_the_attachment_limit_is_refused() {
        let mut setup = setup();
        setup.core.core.set_attachment_limit(4);
        let refused = attach(&setup, &setup.grace, "deploy.txt", HELLO).unwrap_err();
        assert_eq!(refused.kind(), "invalid_input");
        assert!(refused.message().starts_with("This file is larger than 4 bytes"), "{}", refused.message());
        setup.core.core.set_attachment_limit(5);
        attach(&setup, &setup.grace, "deploy.txt", HELLO).unwrap();
    }

    #[test]
    fn only_safe_kinds_of_image_are_shown_as_images() {
        let setup = setup();
        let png = attach(&setup, &setup.grace, "Diagram.PNG", HELLO).unwrap();
        let svg = attach(&setup, &setup.grace, "diagram.svg", HELLO).unwrap();
        assert_eq!(png.content_type, "image/png");
        assert!(png.shown_as_image);
        assert_eq!(svg.content_type, "application/octet-stream");
        assert!(!svg.shown_as_image);
    }
}
