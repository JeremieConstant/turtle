//! The operations layer (ADR-0005): the single place where anything happens.
//!
//! Every capability is an operation: a function that takes the core, the
//! acting principal and typed input, and returns typed output or a typed
//! error. Operations are listed in one catalogue; the HTTP API (and later the
//! MCP tools) are generated from it, so nothing can exist in only one of them.

pub mod accounts;
pub mod areas;
pub mod permissions;

use rusqlite::Connection;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

/// What operations work on: the database and the time.
pub struct Core {
    db: Mutex<Connection>,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl Core {
    pub fn open(db_path: &Path) -> anyhow::Result<Core> {
        Ok(Core {
            db: Mutex::new(crate::store::open(db_path)?),
            clock: Arc::new(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |since| since.as_secs() as i64)
            }),
        })
    }

    pub fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap()
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
    },
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
    areas::register(&mut catalogue);
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

    /// A signed-in Member, created directly because inviting Members is not
    /// an operation yet.
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
        }
    }

    pub fn fresh() -> TestCore {
        let dir = tempfile::tempdir().unwrap();
        let mut core = Core::open(&dir.path().join("turtle.db")).unwrap();
        let time = Arc::new(AtomicI64::new(1_700_000_000));
        let shown = time.clone();
        core.clock = Arc::new(move || shown.load(Ordering::SeqCst));
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
            "create_area",
            "get_area",
            "list_areas",
            "rename_area",
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
