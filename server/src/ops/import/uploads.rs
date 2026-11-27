//! ZIPs uploaded ahead of the import that reads them (ADR-0014). A ZIP sent
//! in base64 with the operation is held in memory several times over, which
//! a whole Confluence space does not fit; an uploaded one is written to a
//! file beside the database as it arrives, and read from there.
//!
//! An upload belongs to whoever sent it. It is kept until an import has
//! read it, or for an hour, and each person has one at a time: a new one
//! takes the place of the last, and only one arrives at a time. So beside
//! the ZIPs their running imports are still reading, a person's uploads
//! take at most twice the import limit on disk. What is known of them is
//! held in memory, so none outlasts Turtle stopping.

use super::running::owner_of;
use crate::ops::{Core, OpError, Principal};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// How long an upload no import has read is kept, in seconds.
const KEPT_FOR: i64 = 60 * 60;

/// The uploads waiting for an import.
#[derive(Default)]
pub struct Uploads {
    known: Mutex<HashMap<String, Known>>,
    /// Who has an upload arriving right now, as `owner_of` names them.
    arriving: Mutex<HashSet<String>>,
}

struct Known {
    /// Who sent it, as `owner_of` names them: only they can import it.
    owner: String,
    path: PathBuf,
    /// When it arrived, in seconds since the Unix epoch.
    received_at: i64,
}

/// An uploaded ZIP, as the upload answers.
#[derive(Debug, Serialize)]
pub struct Uploaded {
    /// What to give an import as `upload_id` in place of `content`.
    pub upload_id: String,
    /// How large the ZIP is, in bytes.
    pub bytes: u64,
}

/// A ZIP while it arrives: written to its file piece by piece, and removed
/// again unless it arrives whole.
pub struct Receiving {
    id: String,
    owner: String,
    path: PathBuf,
    file: Option<File>,
    bytes: u64,
    limit: u64,
    /// Where it is noted that this person has an upload arriving.
    uploads: std::sync::Arc<Uploads>,
}

/// Begins an upload by someone signed in. `announced` is how large the
/// request says the ZIP is, where it does: one too large is refused before
/// any of it is taken.
pub fn receive(core: &Core, principal: &Principal, announced: Option<u64>) -> Result<Receiving, OpError> {
    let owner = owner_of(principal)?;
    // Whoever cannot import has no use for an upload.
    crate::ops::may_change(principal)?;
    let limit = core.import_limit();
    if announced.is_some_and(|bytes| bytes > limit) {
        return Err(super::too_large(core));
    }
    forget_old(core);
    let directory = core.uploads_dir();
    std::fs::create_dir_all(directory).map_err(|error| OpError::Internal(error.into()))?;
    let id = super::record::new_id()?;
    let path = directory.join(&id);
    if !core.uploads().arriving.lock().unwrap().insert(owner.clone()) {
        return Err(OpError::Conflict(
            "Another ZIP of yours is still uploading. Wait until it is done and try again.".into(),
        ));
    }
    // From here on letting go of what is returned says the upload ended.
    let mut receiving = Receiving { id, owner, path, file: None, bytes: 0, limit, uploads: core.uploads_shared() };
    receiving.file = Some(File::create(&receiving.path).map_err(|error| OpError::Internal(error.into()))?);
    Ok(receiving)
}

impl Receiving {
    /// Takes the next piece of the ZIP. Fails once it is larger than this
    /// Turtle imports, however large the request said it was.
    pub fn write(&mut self, core: &Core, piece: &[u8]) -> Result<(), OpError> {
        self.bytes += piece.len() as u64;
        if self.bytes > self.limit {
            return Err(super::too_large(core));
        }
        let Some(file) = self.file.as_mut() else { return Ok(()) };
        file.write_all(piece).map_err(|error| OpError::Internal(error.into()))
    }

    /// Keeps the ZIP, now that all of it arrived, for its sender to import.
    pub fn finish(mut self, core: &Core) -> Result<Uploaded, OpError> {
        let Some(mut file) = self.file.take() else {
            return Err(OpError::Internal(anyhow::anyhow!("the upload was finished twice")));
        };
        file.flush().map_err(|error| OpError::Internal(error.into()))?;
        drop(file);
        let mut known = core.uploads().known.lock().unwrap();
        // One upload each: the last one gives way.
        known.retain(|_, upload| {
            let earlier = upload.owner == self.owner;
            if earlier {
                remove(&upload.path);
            }
            !earlier
        });
        known.insert(self.id.clone(), Known { owner: self.owner.clone(), path: self.path.clone(), received_at: core.now() });
        Ok(Uploaded { upload_id: self.id.clone(), bytes: self.bytes })
    }
}

impl Drop for Receiving {
    fn drop(&mut self) {
        // Still open: it did not arrive whole, or was refused on the way.
        if self.file.take().is_some() {
            remove(&self.path);
        }
        self.uploads.arriving.lock().unwrap().remove(&self.owner);
    }
}

/// An uploaded ZIP opened to read. One taken for an import is removed when
/// this is let go of, however the import ends.
pub struct Opened {
    file: std::io::BufReader<File>,
    /// The file to remove at the end, for an upload an import took.
    taken: Option<PathBuf>,
}

impl Drop for Opened {
    fn drop(&mut self) {
        if let Some(path) = self.taken.take() {
            remove(&path);
        }
    }
}

impl std::io::Read for Opened {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buffer)
    }
}

impl std::io::Seek for Opened {
    fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
        self.file.seek(to)
    }
}

/// What reading an upload is for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum For {
    /// A look at what it holds: it stays, to be imported after.
    Checking,
    /// An import, which uses it up.
    Importing,
}

/// Opens an upload of the principal's. Someone else's answers as one that
/// does not exist.
pub(super) fn open(core: &Core, principal: &Principal, upload_id: &str, purpose: For) -> Result<Opened, OpError> {
    let owner = owner_of(principal)?;
    forget_old(core);
    let mut known = core.uploads().known.lock().unwrap();
    let path = match known.get(upload_id).filter(|upload| upload.owner == owner) {
        Some(upload) => upload.path.clone(),
        None => return Err(gone()),
    };
    let file = File::open(&path).map_err(|error| {
        eprintln!("an upload's file could not be opened: {error}");
        gone()
    })?;
    let taken = (purpose == For::Importing).then(|| {
        known.remove(upload_id);
        path
    });
    Ok(Opened { file: std::io::BufReader::new(file), taken })
}

fn gone() -> OpError {
    OpError::NotFound(
        "There is no such upload. An upload is used up by an import, gives way to a newer one of yours and is kept for an hour at most. Upload the ZIP again.".into(),
    )
}

/// Lets go of the uploads no import has read for an hour.
pub fn forget_old(core: &Core) {
    let now = core.now();
    core.uploads().known.lock().unwrap().retain(|_, upload| {
        let kept = now - upload.received_at < KEPT_FOR;
        if !kept {
            remove(&upload.path);
        }
        kept
    });
}

/// Removes what an earlier run of Turtle left behind: no upload outlasts
/// Turtle stopping, since what is known of them is not kept.
pub fn remove_left_behind(directory: &Path) {
    let _ = std::fs::remove_dir_all(directory);
}

fn remove(path: &Path) {
    // A file already gone is as wanted.
    let _ = std::fs::remove_file(path);
}

/// Uploads a file in two pieces, as it arrives over HTTP, and returns its
/// ID.
#[cfg(test)]
pub fn uploaded(core: &Core, by: &Principal, file: &[u8]) -> String {
    let mut receiving = receive(core, by, Some(file.len() as u64)).unwrap();
    let (first, rest) = file.split_at(file.len() / 2);
    receiving.write(core, first).unwrap();
    receiving.write(core, rest).unwrap();
    let uploaded = receiving.finish(core).unwrap();
    assert_eq!(uploaded.bytes, file.len() as u64);
    uploaded.upload_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::import::{import, ToImport, ZipGiven};
    use crate::ops::testing::{fresh, member};
    use crate::ops::tokens::{create_token, principal_for_token, NewToken};
    use crate::ops::tree::{get_tree, TreeOfArea};

    /// A ZIP holding these Markdown files, by path.
    fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (path, content) in files {
            zip.start_file(*path, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    use super::uploaded as upload;

    /// How many files lie where uploads are kept.
    fn files_kept(core: &Core) -> usize {
        std::fs::read_dir(core.uploads_dir()).map(Iterator::count).unwrap_or(0)
    }

    fn area(core: &Core, admin: &Principal) -> String {
        create_area(core, admin, NewArea { name: "Engineering".into() }).unwrap().output.id
    }

    fn import_upload(core: &Core, by: &Principal, area_id: &str, upload_id: &str) -> Result<Vec<String>, OpError> {
        import(core, by, ToImport { area_id: area_id.into(), parent_id: None, zip: ZipGiven::uploaded(upload_id) })?;
        Ok(get_tree(core, by, TreeOfArea { area_id: area_id.into() }).unwrap().output.entries.into_iter().map(|entry| entry.title).collect())
    }

    #[test]
    fn an_uploaded_zip_is_imported_from_its_file_once_and_then_removed() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace);
        let upload_id = upload(&core, &grace, &zip_of(&[("Runbook.md", "Restart it.\n")]));
        assert_eq!(files_kept(&core), 1);

        let titles = import_upload(&core, &grace, &area_id, &upload_id).unwrap();

        assert_eq!(titles, ["Runbook"]);
        assert_eq!(files_kept(&core), 0);
        let again = import_upload(&core, &grace, &area_id, &upload_id).unwrap_err();
        assert_eq!(again.kind(), "not_found");
        assert_eq!(
            again.message(),
            "There is no such upload. An upload is used up by an import, gives way to a newer one of yours and is kept for an hour at most. Upload the ZIP again."
        );
    }

    #[test]
    fn only_whoever_uploaded_a_zip_can_import_it() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let kens_area = area(&core, &ken);
        let upload_id = upload(&core, &grace, &zip_of(&[("Runbook.md", "")]));

        let refused = import_upload(&core, &ken, &kens_area, &upload_id).unwrap_err();

        // As if it did not exist, and it stays for whoever uploaded it.
        assert_eq!(refused.kind(), "not_found");
        let graces_area = area(&core, &grace);
        assert_eq!(import_upload(&core, &grace, &graces_area, &upload_id).unwrap(), ["Runbook"]);
    }

    #[test]
    fn an_upload_no_import_has_read_is_removed_after_an_hour() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace);
        let upload_id = upload(&core, &grace, &zip_of(&[("Runbook.md", "")]));

        core.advance(KEPT_FOR - 1);
        forget_old(&core);
        assert_eq!(files_kept(&core), 1);
        core.advance(1);
        forget_old(&core);

        assert_eq!(files_kept(&core), 0);
        assert_eq!(import_upload(&core, &grace, &area_id, &upload_id).unwrap_err().kind(), "not_found");
    }

    #[test]
    fn a_new_upload_takes_the_place_of_the_last_one_of_the_same_person() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area_id = area(&core, &grace);
        let first = upload(&core, &grace, &zip_of(&[("First.md", "")]));
        upload(&core, &ken, &zip_of(&[("Kens.md", "")]));

        let second = upload(&core, &grace, &zip_of(&[("Second.md", "")]));

        // Grace's second and Ken's one.
        assert_eq!(files_kept(&core), 2);
        assert_eq!(import_upload(&core, &grace, &area_id, &first).unwrap_err().kind(), "not_found");
        assert_eq!(import_upload(&core, &grace, &area_id, &second).unwrap(), ["Second"]);
    }

    #[test]
    fn a_zip_larger_than_this_turtle_imports_is_refused_however_large_it_said_it_was() {
        let mut core = fresh();
        core.core.set_import_limit(10);
        let grace = member(&core, "Grace", false);

        let announced = receive(&core, &grace, Some(11)).err().unwrap();
        let mut receiving = receive(&core, &grace, Some(4)).unwrap();
        receiving.write(&core, b"123456").unwrap();
        let arrived = receiving.write(&core, b"78901").unwrap_err();
        drop(receiving);

        let told = "This ZIP is larger than 10 bytes, the most this Turtle imports at once. Import it in parts, or ask whoever runs Turtle to raise the limit with TURTLE_MAX_IMPORT_MB.";
        assert_eq!((announced.kind(), announced.message()), ("invalid_input", told));
        assert_eq!((arrived.kind(), arrived.message()), ("invalid_input", told));
        assert_eq!(files_kept(&core), 0);
    }

    #[test]
    fn an_upload_that_breaks_off_leaves_nothing_behind() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let mut receiving = receive(&core, &grace, None).unwrap();
        receiving.write(&core, b"half a ZIP").unwrap();
        assert_eq!(files_kept(&core), 1);

        drop(receiving);

        assert_eq!(files_kept(&core), 0);
    }

    #[test]
    fn an_import_that_is_started_takes_its_upload_at_once_and_a_wrong_one_is_refused_at_once() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace);
        let start = |upload_id: &str| {
            crate::ops::import::start_import(&core, &grace, ToImport { area_id: area_id.clone(), parent_id: None, zip: ZipGiven::uploaded(upload_id) })
        };
        let upload_id = upload(&core, &grace, &zip_of(&[("Runbook.md", "")]));

        let unknown = start("made-up").err().unwrap();
        let started = start(&upload_id).unwrap().output;

        assert_eq!(unknown.kind(), "not_found");
        // This core is not shared, so the import is done once it has answered.
        let progress = crate::ops::import::get_import_progress(&core, &grace, crate::ops::import::running::OfImport { import_id: started.import_id }).unwrap().output;
        assert_eq!(progress.imported.unwrap().entries[0].title, "Runbook");
        assert_eq!(files_kept(&core), 0);
    }

    #[test]
    fn one_zip_of_a_person_arrives_at_a_time() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let first = receive(&core, &grace, None).unwrap();

        let second = receive(&core, &grace, None).err().unwrap();
        let kens = receive(&core, &ken, None);

        assert_eq!(second.kind(), "conflict");
        assert_eq!(second.message(), "Another ZIP of yours is still uploading. Wait until it is done and try again.");
        assert!(kens.is_ok());
        // Once the first has ended, however it ended, the next may begin.
        drop(first);
        assert!(receive(&core, &grace, None).is_ok());
    }

    #[test]
    fn nobody_uploads_without_being_signed_in_or_with_a_read_only_token() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let token = |read_only| {
            let input = NewToken { name: "Claude".into(), read_only, area_ids: None, expires_at: None, agent_id: None };
            principal_for_token(&core, &create_token(&core, &grace, input).unwrap().output.value).unwrap().unwrap()
        };

        let nobody = receive(&core, &Principal::Anonymous, Some(4)).err().unwrap();
        let reader = receive(&core, &token(true), Some(4)).err().unwrap();

        assert_eq!(nobody.kind(), "not_permitted");
        assert_eq!((reader.kind(), reader.message()), ("not_permitted", "This Token is read-only, so it cannot change anything."));
        assert_eq!(files_kept(&core), 0);
        // An Agent whose Token may change things uploads, and imports what it uploaded.
        let agent = token(false);
        let area_id = area(&core, &grace);
        let upload_id = upload(&core, &agent, &zip_of(&[("Runbook.md", "")]));
        assert_eq!(import_upload(&core, &agent, &area_id, &upload_id).unwrap(), ["Runbook"]);
    }

    #[test]
    fn an_upload_that_is_no_zip_is_refused_and_removed() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace);
        let upload_id = upload(&core, &grace, b"not a ZIP at all");

        let refused = import_upload(&core, &grace, &area_id, &upload_id).unwrap_err();

        assert_eq!((refused.kind(), refused.message()), ("invalid_input", "Choose a ZIP file."));
        assert_eq!(files_kept(&core), 0);
    }

    #[test]
    fn an_import_is_given_its_zip_one_way_not_both_and_not_neither() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace);
        let upload_id = upload(&core, &grace, &zip_of(&[("Runbook.md", "")]));
        let both = ZipGiven { content: Some("UEsFBgAAAAAAAAAAAAAAAAAAAAAAAA==".into()), upload_id: Some(upload_id.clone()) };

        let refusals = [both, ZipGiven::default()].map(|zip| import(&core, &grace, ToImport { area_id: area_id.clone(), parent_id: None, zip }).unwrap_err());

        for refused in refusals {
            assert_eq!(refused.kind(), "invalid_input");
            assert_eq!(refused.message(), "Give the ZIP either as content, in base64, or as the upload_id of one uploaded before.");
        }
        // The upload was not touched.
        assert_eq!(import_upload(&core, &grace, &area_id, &upload_id).unwrap(), ["Runbook"]);
    }
}
