//! The operations layer (ADR-0005): the single place where anything happens.
//!
//! Every capability is an operation: a function that takes the core, the
//! acting principal and typed input, and returns typed output or a typed
//! error. Operations are listed in one catalogue; the HTTP API (and later the
//! MCP tools) are generated from it, so nothing can exist in only one of them.

pub mod accounts;
pub mod agents;
pub mod attachments;
pub mod areas;
pub mod connections;
pub mod content;
pub mod export;
pub mod fonts;
#[cfg(test)]
mod every_operation;
pub mod grants;
pub mod intranet;
pub mod links;
pub mod members;
pub mod moves;
pub mod pdf_export;
pub mod pdf_styles;
pub mod permissions;
pub mod profiles;
pub mod quick_notes;
pub mod search;
pub mod tags;
pub mod tokens;
pub mod trash;
pub mod tree;
pub mod versions;

use rusqlite::Connection;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use crate::public_url::PublicUrl;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
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
    /// Where Attachment files are kept, each under the hash of its content.
    attachments: std::path::PathBuf,
    /// Where the files of PDF Styles' logos are kept, each under the hash of
    /// its content.
    pdf_logos: std::path::PathBuf,
    /// Where uploaded font files are kept, each under the hash of its
    /// content. Apart from the Attachments, whose clean-up would remove them.
    fonts: std::path::PathBuf,
    profile_pictures: std::path::PathBuf,
    /// The largest file, in bytes, that can be attached.
    attachment_limit: u64,
    /// The address Turtle is reached at from outside. OAuth is offered only
    /// when it is known (ADR-0010).
    public_url: Option<PublicUrl>,
    /// Looks up host names, to tell whether Turtle's public address is
    /// reachable only from inside its network.
    resolve: intranet::Resolver,
    /// Whether Turtle looks reachable only from inside its network, once it
    /// was found out. The public URL does not change while Turtle runs, so
    /// its host is looked up once.
    intranet_only: OnceLock<bool>,
    /// Sends invitations and password reset links, when SMTP is set up.
    /// Mailed links point at the public URL, so it is set too.
    mailer: Option<Arc<dyn crate::mail::Mailer>>,
    /// Set when this Turtle is a Demo.
    demo: Option<crate::demo::DemoSettings>,
    /// Whether the operator has been told, this start, that the demo
    /// Member is a Workspace Admin and so is not shown.
    demo_warning: std::sync::Once,
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
            attachments: db_path.with_file_name("attachments"),
            pdf_logos: db_path.with_file_name("pdf-logos"),
            fonts: db_path.with_file_name("fonts"),
            profile_pictures: db_path.with_file_name("profile-pictures"),
            attachment_limit: attachments::DEFAULT_ATTACHMENT_LIMIT,
            public_url: None,
            resolve: intranet::system_resolver(),
            intranet_only: OnceLock::new(),
            mailer: None,
            demo: None,
            demo_warning: std::sync::Once::new(),
            clock,
            access: tokio::sync::watch::Sender::new(0),
        })
    }

    /// Sets the largest file, in bytes, that can be attached.
    pub fn set_attachment_limit(&mut self, bytes: u64) {
        self.attachment_limit = bytes;
    }

    pub fn attachment_limit(&self) -> u64 {
        self.attachment_limit
    }

    /// Sets the address Turtle is reached at from outside, which offers OAuth.
    pub fn set_public_url(&mut self, url: Option<PublicUrl>) {
        self.public_url = url;
        self.intranet_only = OnceLock::new();
    }

    /// The address Turtle is reached at from outside, if it is known.
    pub fn public_url(&self) -> Option<&PublicUrl> {
        self.public_url.as_ref()
    }

    /// Sets what sends mail. Without one, links are passed on by hand.
    pub fn set_mailer(&mut self, mailer: Option<Arc<dyn crate::mail::Mailer>>) {
        self.mailer = mailer;
    }

    /// What sends mail, if Turtle sends any.
    pub fn mailer(&self) -> Option<&Arc<dyn crate::mail::Mailer>> {
        self.mailer.as_ref()
    }

    /// Makes this Turtle a Demo, or not.
    pub fn set_demo(&mut self, demo: Option<crate::demo::DemoSettings>) {
        self.demo = demo;
    }

    /// The demo settings, when this Turtle is a Demo.
    pub fn demo(&self) -> Option<&crate::demo::DemoSettings> {
        self.demo.as_ref()
    }

    /// Tells the operator, once per start, that the demo Member is a
    /// Workspace Admin and so is not shown.
    pub fn warn_demo_member_is_workspace_admin(&self) {
        self.demo_warning.call_once(crate::demo::warn_about_workspace_admin);
    }

    /// Sets how host names are looked up, so tests need no network.
    #[cfg(test)]
    pub fn set_resolver(&mut self, resolve: intranet::Resolver) {
        self.resolve = resolve;
        self.intranet_only = OnceLock::new();
    }

    /// Whether Turtle looks reachable only from inside its network, so that
    /// claude.ai could not reach it. The first call may wait for the system
    /// resolver, so it is made where waiting is allowed: in an operation,
    /// which the adapters run off the async workers, or at start-up.
    pub fn looks_intranet_only(&self) -> bool {
        *self.intranet_only.get_or_init(|| intranet::looks_intranet_only(self.public_url(), &self.resolve))
    }

    /// Where Attachment files are kept.
    pub fn attachments_dir(&self) -> &Path {
        &self.attachments
    }

    /// Where the files of PDF Styles' logos are kept.
    pub fn pdf_logos_dir(&self) -> &Path {
        &self.pdf_logos
    }

    /// Where the files of Profile pictures are kept.
    pub fn profile_pictures_dir(&self) -> &Path {
        &self.profile_pictures
    }

    /// Where uploaded font files are kept.
    pub fn fonts_dir(&self) -> &Path {
        &self.fonts
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
    /// A standalone Agent, which always acts through one of its Tokens and
    /// holds only the Roles given to it by name.
    Agent { id: String, token: TokenLimits },
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
            Principal::Agent { token, .. } => Some(token),
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
            Principal::Agent { .. } | Principal::Anonymous => false,
        }
    }
}

/// Refuses everyone but a Workspace Admin, saying what only they can do.
pub(crate) fn require_workspace_admin(principal: &Principal, refusal: &str) -> Result<(), OpError> {
    if principal.acts_as_workspace_admin() {
        Ok(())
    } else {
        Err(OpError::NotPermitted(refusal.into()))
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
    agents::register(&mut catalogue);
    attachments::register(&mut catalogue);
    members::register(&mut catalogue);
    profiles::register(&mut catalogue);
    areas::register(&mut catalogue);
    grants::register(&mut catalogue);
    tree::register(&mut catalogue);
    trash::register(&mut catalogue);
    quick_notes::register(&mut catalogue);
    search::register(&mut catalogue);
    tags::register(&mut catalogue);
    versions::register(&mut catalogue);
    moves::register(&mut catalogue);
    content::register(&mut catalogue);
    export::register(&mut catalogue);
    pdf_export::register(&mut catalogue);
    pdf_styles::register(&mut catalogue);
    fonts::register(&mut catalogue);
    links::register(&mut catalogue);
    tokens::register(&mut catalogue);
    connections::register(&mut catalogue);
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

    /// A signed-in Member with their Personal Area, created directly so a
    /// test needs no invitation.
    pub fn member(core: &Core, name: &str, workspace_admin: bool) -> Principal {
        let id = format!("member-{}", name.to_lowercase());
        let db = core.db();
        db.execute(
            "INSERT INTO members (id, email, name, password_hash, workspace_admin, created_at)
             VALUES (?1, ?2, ?3, 'no password', ?4, 0)",
            rusqlite::params![id, format!("{id}@example.org"), name, workspace_admin],
        )
        .unwrap();
        areas::create_personal_area(&db, &id, 0).unwrap();
        drop(db);
        Principal::Member {
            id,
            workspace_admin,
            session: None,
            token: None,
        }
    }

    /// A signed-in Guest, created directly: Turtle cannot invite Guests yet.
    /// A Guest has no Personal Area.
    pub fn guest(core: &Core, name: &str) -> Principal {
        let id = format!("guest-{}", name.to_lowercase());
        core.db()
            .execute(
                "INSERT INTO members (id, email, name, password_hash, workspace_admin, guest, created_at)
                 VALUES (?1, ?2, ?3, 'no password', 0, 1, 0)",
                rusqlite::params![id, format!("{id}@example.org"), name],
            )
            .unwrap();
        Principal::Member {
            id,
            workspace_admin: false,
            session: None,
            token: None,
        }
    }

    /// The ID of a Member's Personal Area, as they see it listed first.
    pub fn personal_area(core: &Core, of: &Principal) -> String {
        let list = areas::list_areas(core, of, accounts::Nothing {}).unwrap().output;
        let first = list.areas.into_iter().next().expect("a Member has a Personal Area");
        assert!(first.personal, "the Personal Area is listed first");
        first.id
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

    /// Takes away the Grant a Member has on an Area.
    pub fn take_role(core: &Core, area_id: &str, from: &Principal) {
        let Principal::Member { id, .. } = from else { panic!("only Members get Grants") };
        let db = core.db();
        let root = tree::root_of(&db, area_id).unwrap();
        permissions::revoke(&db, &root, &permissions::Holder::Member(id.clone())).unwrap();
    }

    /// The styles a test font comes in, each made from Inter's file of that style.
    #[derive(Debug, Clone, Copy)]
    pub enum TestStyle {
        Regular,
        Bold,
        Italic,
        BoldItalic,
    }

    /// A font file of the family `family` in `style`: one of Inter's files
    /// under another name, so a test can upload a family Turtle lacks.
    pub fn font_file(family: &str, style: TestStyle) -> Vec<u8> {
        let (source, subfamily): (&[u8], &str) = match style {
            TestStyle::Regular => (include_bytes!("../../fonts/inter/Inter-Regular.ttf"), "Regular"),
            TestStyle::Bold => (include_bytes!("../../fonts/inter/Inter-Bold.ttf"), "Bold"),
            TestStyle::Italic => (include_bytes!("../../fonts/inter/Inter-Italic.ttf"), "Italic"),
            TestStyle::BoldItalic => (include_bytes!("../../fonts/inter/Inter-BoldItalic.ttf"), "Bold Italic"),
        };
        let postscript = format!("{}-{}", family.replace(' ', ""), subfamily.replace(' ', ""));
        let names = [
            (1, family.to_string()),
            (2, subfamily.to_string()),
            (4, format!("{family} {subfamily}")),
            (6, postscript),
        ];
        let mut name = Vec::new();
        let mut strings = Vec::new();
        name.extend(0u16.to_be_bytes());
        name.extend((names.len() as u16).to_be_bytes());
        name.extend((6 + 12 * names.len() as u16).to_be_bytes());
        for (id, text) in names {
            let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
            // Windows, Unicode BMP, US English.
            for field in [3u16, 1, 0x409, id, utf16.len() as u16, strings.len() as u16] {
                name.extend(field.to_be_bytes());
            }
            strings.extend(utf16);
        }
        name.extend(strings);
        with_tables(source, &[(*b"name", name)])
    }

    /// The font with a variation axis for weight, as a variable font has.
    pub fn variable(font: &[u8]) -> Vec<u8> {
        let mut fvar = Vec::new();
        // Version 1.0, axes at 16, 2 reserved, 1 axis of 20 bytes, no instances of 8 bytes.
        for field in [1u16, 0, 16, 2, 1, 20, 0, 8] {
            fvar.extend(field.to_be_bytes());
        }
        fvar.extend(b"wght");
        for value in [100u32 << 16, 400 << 16, 900 << 16] {
            fvar.extend(value.to_be_bytes());
        }
        fvar.extend([0u8, 0, 1, 0]);
        with_tables(font, &[(*b"fvar", fvar)])
    }

    /// The font file with these tables put in or replaced.
    fn with_tables(font: &[u8], replaced: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
        let read_u16 = |at: usize| u16::from_be_bytes([font[at], font[at + 1]]);
        let read_u32 = |at: usize| u32::from_be_bytes(font[at..at + 4].try_into().unwrap());
        let mut tables: std::collections::BTreeMap<[u8; 4], Vec<u8>> = (0..usize::from(read_u16(4)))
            .map(|index| {
                let record = 12 + 16 * index;
                let tag: [u8; 4] = font[record..record + 4].try_into().unwrap();
                let (offset, length) = (read_u32(record + 8) as usize, read_u32(record + 12) as usize);
                (tag, font[offset..offset + length].to_vec())
            })
            .collect();
        for (tag, table) in replaced {
            tables.insert(*tag, table.clone());
        }
        let count = tables.len() as u16;
        let entry_selector = 15 - count.leading_zeros() as u16;
        let search_range = 16u16 << entry_selector;
        let mut out = font[0..4].to_vec();
        for field in [count, search_range, entry_selector, count * 16 - search_range] {
            out.extend(field.to_be_bytes());
        }
        let mut offset = 12 + 16 * tables.len();
        let mut data = Vec::new();
        for (tag, table) in &tables {
            let mut padded = table.clone();
            padded.resize(table.len().div_ceil(4) * 4, 0);
            let checksum = padded
                .chunks(4)
                .fold(0u32, |sum, word| sum.wrapping_add(u32::from_be_bytes(word.try_into().unwrap())));
            out.extend(tag);
            out.extend(checksum.to_be_bytes());
            out.extend((offset as u32).to_be_bytes());
            out.extend((table.len() as u32).to_be_bytes());
            offset += padded.len();
            data.extend(padded);
        }
        out.extend(data);
        out
    }

    /// A fresh core that mails through a fake mailer, at a public URL.
    pub fn fresh_with_mail(mailer: crate::mail::fake::FakeMailer) -> (TestCore, Arc<crate::mail::fake::FakeMailer>) {
        let mut core = fresh();
        let mailer = Arc::new(mailer);
        core.core.set_public_url(Some(crate::public_url::PublicUrl::parse("https://turtle.example.org").unwrap()));
        core.core.set_mailer(Some(mailer.clone()));
        (core, mailer)
    }

    pub fn fresh() -> TestCore {
        let dir = tempfile::tempdir().unwrap();
        let time = Arc::new(AtomicI64::new(1_700_000_000));
        let shown = time.clone();
        let mut core =
            Core::open_with_clock(&dir.path().join("turtle.db"), Arc::new(move || shown.load(Ordering::SeqCst)))
                .unwrap();
        // Tests look nothing up on the network.
        core.set_resolver(Arc::new(|_| Vec::new()));
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
        // Which operations exist is held by the table in `every_operation`,
        // which needs a row for each.
        let catalogue = catalogue();
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
        assert_eq!(reply.output, json!({"setup_required": true, "sends_mail": false, "member": null}));
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
