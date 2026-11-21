//! The operations layer (ADR-0005): the single place where anything happens.
//!
//! Every capability is an operation: a function that takes the core, the
//! acting principal and typed input, and returns typed output or a typed
//! error. Operations are listed in one catalogue; the HTTP API (and later the
//! MCP tools) are generated from it, so nothing can exist in only one of them.

pub mod accounts;
pub mod areas;
#[cfg(test)]
mod every_operation;
pub mod grants;
pub mod members;
pub mod permissions;
pub mod search;
pub mod tokens;
pub mod tree;
pub mod versions;

use rusqlite::Connection;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

/// Tells the time in seconds since the Unix epoch.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// What operations work on: the database and the time.
pub struct Core {
    db: Mutex<Connection>,
    clock: Clock,
    /// Counts the changes that may have taken access away or changed a Role,
    /// so open editors check theirs again at once.
    access: tokio::sync::watch::Sender<u64>,
    search: crate::search::Index,
    live: crate::live::Pages,
}

impl Core {
    /// Opens the database and, beside it, the search index.
    pub fn open(db_path: &Path) -> anyhow::Result<Core> {
        Core::open_with_clock(
            db_path,
            Arc::new(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |since| since.as_secs() as i64)
            }),
        )
    }

    fn open_with_clock(db_path: &Path, clock: Clock) -> anyhow::Result<Core> {
        // The open Pages get a connection of their own, so live editing does
        // not wait behind operations.
        Ok(Core {
            db: Mutex::new(crate::store::open(db_path)?),
            search: crate::search::Index::open(&db_path.with_file_name("search"))?,
            live: crate::live::Pages::new(crate::store::open(db_path)?, clock.clone()),
            clock,
            access: tokio::sync::watch::Sender::new(0),
        })
    }

    pub fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap()
    }

    /// Says that someone's access may have changed: a Grant, a session, or
    /// whether a Member is active or Workspace Admin.
    pub fn access_changed(&self) {
        self.access.send_modify(|changes| *changes += 1);
    }

    /// Notices every `access_changed` from now on.
    pub fn watch_access(&self) -> tokio::sync::watch::Receiver<u64> {
        self.access.subscribe()
    }

    /// Says that a Page's title, content or place changed, so search finds
    /// it as it is now once the index has caught up.
    pub fn page_changed(&self, page_id: &str) {
        self.search.page_changed(page_id);
    }

    /// The Pages open in memory.
    pub fn live(&self) -> &crate::live::Pages {
        &self.live
    }

    /// Writes every changed open Page to the database and returns how many,
    /// and records a Version of each whose changes have waited long enough.
    pub fn save_changed_pages(&self) -> anyhow::Result<usize> {
        // Recording does not wait for saving: a Version holds its own content.
        let saved = self.live.save_changed(|id| self.page_changed(id));
        self.live.record_due()?;
        saved
    }

    /// Saves every open Page and records its unrecorded changes, as Turtle stops.
    pub fn save_before_stopping(&self) -> anyhow::Result<()> {
        self.live.save_changed(|id| self.page_changed(id))?;
        self.live.record_all()
    }

    /// Brings the search index up to date with every change said so far,
    /// rebuilding it from the database when it was missing.
    pub fn catch_up_search(&self) -> anyhow::Result<()> {
        let pending = self.search.take_pending();
        if pending.is_empty() {
            return Ok(());
        }
        let changes = search::changes(&self.db(), &pending);
        let applied = changes.and_then(|changes| self.search.apply(changes));
        if applied.is_err() {
            self.search.restore_pending(pending);
        }
        applied
    }

    /// Takes in the last changes before Turtle stops, so the next start can
    /// use the index as it is instead of rebuilding it.
    pub fn close_search(&self) -> anyhow::Result<()> {
        self.catch_up_search()?;
        self.search.close()
    }

    /// Seconds since the Unix epoch.
    pub fn now(&self) -> i64 {
        (self.clock)()
    }
}

/// Who is acting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// Nobody is signed in.
    Anonymous,
    Member {
        id: String,
        workspace_admin: bool,
        /// The session this Member is acting through, if any.
        session: Option<SessionId>,
        /// The Token this Member is acting through, if any, with its limits.
        token: Option<TokenLimits>,
    },
}

/// What a Token takes away from its owner's access. It never adds any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenLimits {
    pub id: String,
    /// Nothing can be changed through it.
    pub read_only: bool,
    /// The only Areas it reaches; every Area its owner reaches if `None`.
    pub areas: Option<Vec<String>>,
}

impl Principal {
    /// The limits of the Token the caller acts through, if any.
    pub fn token(&self) -> Option<&TokenLimits> {
        match self {
            Principal::Member { token, .. } => token.as_ref(),
            Principal::Anonymous => None,
        }
    }

    /// Whether the caller may manage Members. A Token limited to chosen Areas
    /// does not carry Workspace Admin standing, which reaches beyond them.
    pub fn acts_as_workspace_admin(&self) -> bool {
        match self {
            Principal::Member { workspace_admin, token, .. } => {
                *workspace_admin && token.as_ref().is_none_or(|token| token.areas.is_none())
            }
            Principal::Anonymous => false,
        }
    }
}

/// Identifies a session in the database. Not the secret the browser holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionId(pub(crate) String);

/// Why an operation did not happen. These four kinds are all a caller ever has
/// to tell apart; `Internal` means Turtle itself failed.
#[derive(Debug)]
pub enum OpError {
    NotFound(String),
    NotPermitted(String),
    InvalidInput(String),
    Conflict(String),
    Internal(anyhow::Error),
}

impl OpError {
    /// The name of the kind as it appears in the API.
    pub fn kind(&self) -> &'static str {
        match self {
            OpError::NotFound(_) => "not_found",
            OpError::NotPermitted(_) => "not_permitted",
            OpError::InvalidInput(_) => "invalid_input",
            OpError::Conflict(_) => "conflict",
            OpError::Internal(_) => "internal",
        }
    }

    /// What to tell the caller. Internal failures are not described to them.
    pub fn message(&self) -> &str {
        match self {
            OpError::NotFound(message)
            | OpError::NotPermitted(message)
            | OpError::InvalidInput(message)
            | OpError::Conflict(message) => message,
            OpError::Internal(_) => "Something went wrong inside Turtle.",
        }
    }
}

impl From<rusqlite::Error> for OpError {
    fn from(error: rusqlite::Error) -> Self {
        OpError::Internal(error.into())
    }
}

impl From<anyhow::Error> for OpError {
    fn from(error: anyhow::Error) -> Self {
        OpError::Internal(error)
    }
}

/// What an operation does to the caller's session, besides returning output.
/// Only the browser-only operations ever change it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionChange {
    Keep,
    /// A session was started; the secret goes into the caller's cookie.
    Start(String),
    End,
}

#[derive(Debug)]
pub struct Reply<T> {
    pub output: T,
    pub session: SessionChange,
}

impl<T> From<T> for Reply<T> {
    fn from(output: T) -> Self {
        Reply {
            output,
            session: SessionChange::Keep,
        }
    }
}

pub type OpResult<T> = Result<Reply<T>, OpError>;

type Run = Box<dyn Fn(&Core, &Principal, Value) -> OpResult<Value> + Send + Sync>;

/// One entry of the catalogue.
pub struct Operation {
    pub name: &'static str,
    pub description: &'static str,
    /// Only makes sense with a browser session, so it gets no MCP tool.
    pub browser_only: bool,
    /// Changes something, so a read-only Token cannot call it. Operations
    /// that only read say so when they are registered.
    pub changes: bool,
    pub input_schema: Value,
    pub output_schema: Value,
    run: Run,
}

/// Every operation Turtle has.
pub struct Catalogue {
    operations: Vec<Operation>,
}

impl Catalogue {
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }

    /// Every operation with its schemas, as JSON. The web app's types are
    /// generated from this.
    pub fn describe(&self) -> Value {
        let operations: Vec<Value> = self
            .operations()
            .iter()
            .map(|operation| {
                serde_json::json!({
                    "name": operation.name,
                    "description": operation.description,
                    "browser_only": operation.browser_only,
                    "changes": operation.changes,
                    "input": operation.input_schema,
                    "output": operation.output_schema,
                })
            })
            .collect();
        serde_json::json!({ "operations": operations })
    }

    /// Runs an operation by name with JSON input, as the adapters do.
    pub fn call(
        &self,
        core: &Core,
        principal: &Principal,
        name: &str,
        input: Value,
    ) -> OpResult<Value> {
        let operation = self
            .operations
            .iter()
            .find(|operation| operation.name == name)
            .ok_or_else(|| OpError::NotFound(format!("There is no operation named {name}.")))?;
        if let Some(token) = principal.token() {
            // Browser-only operations include managing Tokens, so a Token can
            // never make another that reaches further.
            if operation.browser_only {
                return Err(OpError::NotPermitted(
                    "This can only be done signed in to Turtle in a browser, not with a Token.".into(),
                ));
            }
            if token.read_only && operation.changes {
                return Err(OpError::NotPermitted(
                    "This Token is read-only, so it cannot change anything.".into(),
                ));
            }
        }
        (operation.run)(core, principal, input)
    }

    fn add<I, O>(
        &mut self,
        name: &'static str,
        description: &'static str,
        run: fn(&Core, &Principal, I) -> OpResult<O>,
    ) -> &mut Operation
    where
        I: DeserializeOwned + JsonSchema + 'static,
        O: Serialize + JsonSchema + 'static,
    {
        assert!(
            self.operations.iter().all(|known| known.name != name),
            "operation {name} is registered twice"
        );
        self.operations.push(Operation {
            name,
            description,
            browser_only: false,
            changes: true,
            input_schema: schemars::schema_for!(I).to_value(),
            output_schema: schemars::schema_for!(O).to_value(),
            run: Box::new(move |core, principal, input| {
                let input: I = serde_json::from_value(input)
                    .map_err(|error| OpError::InvalidInput(error.to_string()))?;
                let reply = run(core, principal, input)?;
                Ok(Reply {
                    output: serde_json::to_value(reply.output).map_err(anyhow::Error::from)?,
                    session: reply.session,
                })
            }),
        });
        self.operations.last_mut().unwrap()
    }
}

/// The one catalogue. Adding a capability to Turtle means adding a line here.
pub fn catalogue() -> Catalogue {
    let mut catalogue = Catalogue {
        operations: Vec::new(),
    };
    accounts::register(&mut catalogue);
    members::register(&mut catalogue);
    areas::register(&mut catalogue);
    grants::register(&mut catalogue);
    tree::register(&mut catalogue);
    search::register(&mut catalogue);
    versions::register(&mut catalogue);
    tokens::register(&mut catalogue);
    catalogue
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    /// A core on a fresh database in its own temporary directory.
    pub struct TestCore {
        pub core: Core,
        time: Arc<AtomicI64>,
        _dir: tempfile::TempDir,
    }

    impl TestCore {
        /// Lets time pass; the test clock never moves on its own.
        pub fn advance(&self, seconds: i64) {
            self.time.fetch_add(seconds, Ordering::SeqCst);
        }
    }

    impl std::ops::Deref for TestCore {
        type Target = Core;
        fn deref(&self) -> &Core {
            &self.core
        }
    }

    /// A signed-in Member, created directly so a test needs no invitation.
    pub fn member(core: &Core, name: &str, workspace_admin: bool) -> Principal {
        let id = format!("member-{}", name.to_lowercase());
        core.db()
            .execute(
                "INSERT INTO members (id, email, name, password_hash, workspace_admin, created_at)
                 VALUES (?1, ?2, ?3, 'no password', ?4, 0)",
                rusqlite::params![id, format!("{id}@example.org"), name, workspace_admin],
            )
            .unwrap();
        Principal::Member {
            id,
            workspace_admin,
            session: None,
            token: None,
        }
    }

    /// Gives a Member a Role on an Area. Grants are not an operation yet.
    pub fn give_role(core: &Core, area_id: &str, with: &Principal, role: permissions::Role) {
        let Principal::Member { id, .. } = with else { panic!("only Members get Grants") };
        let db = core.db();
        let root: String = db
            .query_row(
                "SELECT id FROM nodes WHERE area_id = ?1 AND parent_id IS NULL",
                [area_id],
                |row| row.get(0),
            )
            .unwrap();
        permissions::grant(&db, &root, &permissions::Holder::Member(id.clone()), role).unwrap();
    }

    pub fn fresh() -> TestCore {
        let dir = tempfile::tempdir().unwrap();
        let time = Arc::new(AtomicI64::new(1_700_000_000));
        let shown = time.clone();
        let core =
            Core::open_with_clock(&dir.path().join("turtle.db"), Arc::new(move || shown.load(Ordering::SeqCst)))
                .unwrap();
        TestCore {
            core,
            time,
            _dir: dir,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_operation_is_described_in_the_catalogue() {
        let catalogue = catalogue();
        let names: Vec<_> = catalogue.operations().iter().map(|op| op.name).collect();
        let expected_names = [
            "get_session",
            "create_first_account",
            "sign_in",
            "sign_out",
            "invite_member",
            "accept_invitation",
            "list_members",
            "deactivate_member",
            "set_workspace_admin",
            "create_password_reset",
            "reset_password",
            "create_area",
            "get_area",
            "list_areas",
            "rename_area",
            "create_page",
            "create_folder",
            "rename_page",
            "rename_folder",
            "get_page",
            "get_tree",
            "search",
            "list_versions",
            "get_version",
            "compare_versions",
            "restore_version",
        ];
        for expected in expected_names {
            assert!(names.contains(&expected), "{expected} is missing from {names:?}");
        }
        for operation in catalogue.operations() {
            assert!(!operation.description.is_empty(), "{}", operation.name);
            assert!(operation.input_schema.is_object(), "{}", operation.name);
            assert!(operation.output_schema.is_object(), "{}", operation.name);
        }
    }

    #[test]
    fn operations_are_called_by_name_with_json() {
        let core = testing::fresh();
        let reply = catalogue()
            .call(&core, &Principal::Anonymous, "get_session", json!({}))
            .unwrap();
        assert_eq!(reply.output, json!({"setup_required": true, "member": null}));
    }

    #[test]
    fn an_unknown_operation_is_not_found_and_malformed_input_is_invalid() {
        let core = testing::fresh();
        let catalogue = catalogue();
        let unknown = catalogue
            .call(&core, &Principal::Anonymous, "launch_rockets", json!({}))
            .unwrap_err();
        assert_eq!(unknown.kind(), "not_found");
        let malformed = catalogue
            .call(&core, &Principal::Anonymous, "sign_in", json!({"email": 5}))
            .unwrap_err();
        assert_eq!(malformed.kind(), "invalid_input");
    }
}
