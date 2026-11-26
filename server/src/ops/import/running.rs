//! Imports that go on after the operation that started them has answered,
//! so a large one can be followed while it runs instead of only waited for,
//! and no request has to stay open until it is done.
//!
//! What is known of them is held in memory: an import still running when
//! Turtle stops ends there, keeping what it created so far, and one that
//! ended is forgotten an hour later.

use super::confluence::Coverage;
use super::{Imported, Step};
use crate::ops::{Core, OpError, OpResult, Principal};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

/// How long an import is remembered after it ended, in seconds.
const KEPT_FOR: i64 = 60 * 60;

/// The imports running, and those that ended within the last hour.
#[derive(Default)]
pub struct Imports {
    known: Mutex<HashMap<String, Known>>,
}

struct Known {
    /// Who started it, as `owner_of` names them: only they can follow it.
    owner: String,
    progress: ImportProgress,
    /// When it ended, in seconds since the Unix epoch.
    ended_at: Option<i64>,
}

/// Which import to ask about.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct OfImport {
    /// The ID start_import or start_import_confluence answered with.
    pub import_id: String,
}

/// How far an import has got.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ImportProgress {
    pub import_id: String,
    pub stage: Stage,
    /// How much of the stage is done: Pages and Folders created while
    /// creating, Pages whose content is written while writing.
    pub done: u32,
    /// How much there is to do in the stage; 0 while reading.
    pub total: u32,
    /// A Confluence import's report on the space, as soon as it is read and
    /// before anything is created: what comes across and how well, and which
    /// attachments are too large for this Turtle. Absent for other imports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<Coverage>,
    /// Once it is done: what was created and what did not come across.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported: Option<Imported>,
    /// Once it failed: why, as a sentence. What it created before is kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What an import is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Unpacking the ZIP and reading what it holds.
    Reading,
    /// Creating the Pages and Folders, with their Attachments and Tags.
    Creating,
    /// Writing each Page's content, its links pointed at what was imported.
    Writing,
    Done,
    Failed,
}

/// What a running import tells as it goes.
pub(super) struct Following<'a> {
    imports: &'a Imports,
    import_id: &'a str,
}

impl Following<'_> {
    /// Tells a step of writing what was read.
    pub fn step(&self, step: Step) {
        let (stage, done, total) = match step {
            Step::Creating { done, total } => (Stage::Creating, done, total),
            Step::Writing { done, total } => (Stage::Writing, done, total),
        };
        self.imports.change(self.import_id, |progress| {
            progress.stage = stage;
            progress.done = done;
            progress.total = total;
        });
    }

    /// Tells the report on a Confluence space, once it is read.
    pub fn read(&self, coverage: &Coverage) {
        self.imports.change(self.import_id, |progress| progress.coverage = Some(coverage.clone()));
    }
}

/// Starts an import whose `work` reads its source and writes it, after the
/// operation has answered with its ID. The caller has checked access.
pub(super) fn start(
    core: &Core,
    principal: &Principal,
    work: impl FnOnce(&Core, &Principal, &Following) -> Result<Imported, OpError> + Send + 'static,
) -> OpResult<ImportProgress> {
    let owner = owner_of(principal)?;
    let import_id = crate::ops::accounts::random_hex(16)?;
    let progress = ImportProgress {
        import_id: import_id.clone(),
        stage: Stage::Reading,
        done: 0,
        total: 0,
        coverage: None,
        imported: None,
        error: None,
    };
    let imports = core.imports();
    imports.forget_old(core.now());
    imports.known.lock().unwrap().insert(import_id.clone(), Known { owner, progress: progress.clone(), ended_at: None });
    let principal = principal.clone();
    core.in_background(move |core| {
        let imports = core.imports();
        let following = Following { imports, import_id: &import_id };
        // A panic ends the import as a failure rather than leaving it
        // running for ever.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(core, &principal, &following)))
            .unwrap_or_else(|_| Err(OpError::Internal(anyhow::anyhow!("the import panicked"))));
        if let Err(OpError::Internal(cause)) = &outcome {
            eprintln!("import failed: {cause:#}");
        }
        let now = core.now();
        let mut known = imports.known.lock().unwrap();
        if let Some(known) = known.get_mut(&import_id) {
            match outcome {
                Ok(imported) => {
                    known.progress.stage = Stage::Done;
                    known.progress.imported = Some(imported);
                }
                Err(error) => {
                    known.progress.stage = Stage::Failed;
                    known.progress.error = Some(error.message().to_string());
                }
            }
            known.ended_at = Some(now);
        }
    });
    Ok(progress.into())
}

/// How far an import of the principal's has got.
pub(super) fn progress(core: &Core, principal: &Principal, import_id: &str) -> OpResult<ImportProgress> {
    let owner = owner_of(principal)?;
    let imports = core.imports();
    imports.forget_old(core.now());
    let known = imports.known.lock().unwrap();
    // Someone else's import answers as one that does not exist.
    match known.get(import_id).filter(|known| known.owner == owner) {
        Some(known) => Ok(known.progress.clone().into()),
        None => Err(OpError::NotFound(
            "There is no such import. An import is forgotten an hour after it ends; look for what it brought in where it went.".into(),
        )),
    }
}

/// Who follows an import: the Member or standalone Agent who started it,
/// whichever session or Token they use.
fn owner_of(principal: &Principal) -> Result<String, OpError> {
    crate::ops::areas::signed_in(principal)?;
    Ok(match principal {
        Principal::Member { id, .. } => format!("member:{id}"),
        Principal::Agent { id, .. } => format!("agent:{id}"),
        Principal::Anonymous => unreachable!("signed_in refuses nobody signed in"),
    })
}

impl Imports {
    fn change(&self, import_id: &str, change: impl FnOnce(&mut ImportProgress)) {
        if let Some(known) = self.known.lock().unwrap().get_mut(import_id) {
            change(&mut known.progress);
        }
    }

    fn forget_old(&self, now: i64) {
        self.known.lock().unwrap().retain(|_, known| known.ended_at.is_none_or(|ended| now - ended < KEPT_FOR));
    }
}

#[cfg(test)]
mod tests {
    use super::super::{get_import_progress, start_import, ToImport};
    use super::*;
    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::testing::{fresh, member};
    use crate::ops::tree::{get_tree, TreeOfArea};
    use base64::Engine;
    use std::io::Write;

    fn zip_of(files: &[(&str, &str)]) -> String {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (path, content) in files {
            zip.start_file(*path, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        base64::engine::general_purpose::STANDARD.encode(zip.finish().unwrap().into_inner())
    }

    fn progress_of(core: &Core, principal: &Principal, import_id: &str) -> Result<ImportProgress, OpError> {
        get_import_progress(core, principal, OfImport { import_id: import_id.into() }).map(|reply| reply.output)
    }

    /// Asks until the import has ended, as a client does.
    fn ended(core: &Core, principal: &Principal, import_id: &str) -> ImportProgress {
        for _ in 0..1000 {
            let progress = progress_of(core, principal, import_id).unwrap();
            if matches!(progress.stage, Stage::Done | Stage::Failed) {
                return progress;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the import did not end");
    }

    #[test]
    fn a_started_import_answers_before_it_is_done_and_is_followed_until_it_is() {
        let core = fresh().shared();
        let grace = member(&core, "Grace", false);
        let area_id = create_area(&core, &grace, NewArea { name: "Engineering".into() }).unwrap().output.id;

        let content = zip_of(&[("Runbook.md", "Restart it.\n"), ("Runbook/Deploy.md", "Ship it.\n")]);
        let started = start_import(&core, &grace, ToImport { area_id: area_id.clone(), parent_id: None, content }).unwrap().output;

        assert_eq!(started.stage, Stage::Reading);
        let ended = ended(&core, &grace, &started.import_id);
        assert_eq!(ended.stage, Stage::Done);
        assert_eq!((ended.done, ended.total), (2, 2));
        let imported = ended.imported.unwrap();
        assert_eq!(imported.entries[0].title, "Runbook");
        assert!(imported.problems.is_empty());
        let tree = get_tree(&core, &grace, TreeOfArea { area_id }).unwrap().output.entries;
        assert_eq!(tree[0].children[0].title, "Deploy");
    }

    #[test]
    fn an_import_that_fails_once_running_says_why() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = create_area(&core, &grace, NewArea { name: "Engineering".into() }).unwrap().output.id;

        let started = start_import(&core, &grace, ToImport { area_id, parent_id: None, content: "bm90IGEgemlw".into() }).unwrap().output;

        let ended = ended(&core, &grace, &started.import_id);
        assert_eq!(ended.stage, Stage::Failed);
        assert_eq!(ended.error.as_deref(), Some("Choose a ZIP file."));
    }

    #[test]
    fn an_import_where_the_caller_may_not_write_is_refused_before_it_starts() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area_id = create_area(&core, &grace, NewArea { name: "Engineering".into() }).unwrap().output.id;

        let refused = start_import(&core, &ken, ToImport { area_id, parent_id: None, content: zip_of(&[("A.md", "")]) });

        assert_eq!(refused.unwrap_err().kind(), "not_found");
    }

    #[test]
    fn only_whoever_started_an_import_can_follow_it() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area_id = create_area(&core, &grace, NewArea { name: "Engineering".into() }).unwrap().output.id;
        let started = start_import(&core, &grace, ToImport { area_id, parent_id: None, content: zip_of(&[("A.md", "")]) }).unwrap().output;

        let someone_else = progress_of(&core, &ken, &started.import_id).unwrap_err();
        let made_up = progress_of(&core, &ken, "made-up").unwrap_err();

        assert_eq!(someone_else.kind(), "not_found");
        assert_eq!(someone_else.message(), made_up.message());
        assert_eq!(progress_of(&core, &grace, &started.import_id).unwrap().stage, Stage::Done);
    }

    #[test]
    fn an_import_is_forgotten_an_hour_after_it_ended() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = create_area(&core, &grace, NewArea { name: "Engineering".into() }).unwrap().output.id;
        let started = start_import(&core, &grace, ToImport { area_id, parent_id: None, content: zip_of(&[("A.md", "")]) }).unwrap().output;

        core.advance(KEPT_FOR - 1);
        assert_eq!(progress_of(&core, &grace, &started.import_id).unwrap().stage, Stage::Done);
        core.advance(1);
        assert_eq!(progress_of(&core, &grace, &started.import_id).unwrap_err().kind(), "not_found");
    }
}
