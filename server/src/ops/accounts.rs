//! Accounts and sessions: the first account, signing in and signing out.

use super::{attempts, Catalogue, Client, Core, OpError, OpResult, Principal, Reply, SessionChange, SessionId};
use anyhow::anyhow;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::Argon2;
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "get_session",
            "Says who is signed in, or which standalone Agent is calling, and whether this installation still needs its first account.",
            get_session,
        )
        .changes = false;
    catalogue
        .add(
            "create_first_account",
            "Creates the first Member of a fresh installation, makes them Workspace Admin and signs them in.",
            create_first_account,
        )
        .browser_only = true;
    catalogue
        .add_from_client(
            "sign_in",
            "Signs a Member in with email and password.",
            sign_in,
        )
        .browser_only = true;
    catalogue
        .add("sign_out", "Ends the caller's session.", sign_out)
        .browser_only = true;
}

/// Input of operations that need none.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct Nothing {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Member {
    pub id: String,
    pub name: String,
    pub email: String,
    pub workspace_admin: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Session {
    /// True until the first account has been created.
    pub setup_required: bool,
    /// Whether Turtle sends mail: invitations and password reset links are
    /// then mailed, and a Member who forgot their password can ask for a
    /// reset link when signing in.
    pub sends_mail: bool,
    /// The signed-in Member, if anyone is signed in.
    pub member: Option<Member>,
    /// The standalone Agent calling through one of its Tokens, if it is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<CallingAgent>,
    /// Present when this Turtle is a Demo, which is reset from time to time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub demo: Option<Demo>,
    /// Which Release of Turtle this is, told only to those signed in: a
    /// stranger would learn which Release to attack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<crate::release::Release>,
    /// The largest file and the largest ZIP this Turtle accepts, so a client
    /// can refuse a larger one before sending it. Told only to those signed in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<Limits>,
}

/// The largest files this Turtle accepts, which whoever runs it sets.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Limits {
    /// The largest file, in bytes, that can be attached to a Page, also when
    /// it comes in with an import. Set with `TURTLE_MAX_ATTACHMENT_MB`.
    pub attachment_bytes: u64,
    /// The largest ZIP, in bytes, that can be imported at once. Set with
    /// `TURTLE_MAX_IMPORT_MB`.
    pub import_bytes: u64,
}

/// What a Demo tells everyone, signed in or not.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Demo {
    /// How many hours pass between resets, if the operator said.
    pub resets_every_hours: Option<u32>,
    /// The email address and password of the demo Member, which anyone may
    /// sign in with. Null while no active Member has the Demo's email
    /// address, and when that Member is a Workspace Admin.
    pub account: Option<Credentials>,
}

/// A standalone Agent as it learns who it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CallingAgent {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewAccount {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

const MIN_PASSWORD_LENGTH: usize = 10;
/// Hashing is slow on purpose, so an endless password would tie up the server.
const MAX_PASSWORD_LENGTH: usize = 1024;

/// How long a session lasts without signing in again.
pub const SESSION_LIFETIME: i64 = 30 * 24 * 60 * 60;

pub fn get_session(core: &Core, principal: &Principal, _: Nothing) -> OpResult<Session> {
    let db = core.db();
    let agent = match principal {
        Principal::Agent { id, .. } => Some(db.query_row("SELECT id, name FROM agents WHERE id = ?1", [id], |row| {
            Ok(CallingAgent { id: row.get(0)?, name: row.get(1)? })
        })?),
        _ => None,
    };
    let member = match principal {
        Principal::Anonymous | Principal::Agent { .. } => None,
        Principal::Member { id, .. } => Some(
            db.query_row(
                "SELECT id, name, email, workspace_admin FROM members WHERE id = ?1",
                [id],
                read_member,
            )?,
        ),
    };
    Ok(Session {
        setup_required: !any_member_exists(&db)?,
        sends_mail: core.mailer().is_some(),
        member,
        agent,
        demo: core.demo().map(|settings| demo_for_everyone(core, &db, settings)).transpose()?,
        release: (!matches!(principal, Principal::Anonymous)).then(|| crate::release::RELEASE.clone()),
        limits: (!matches!(principal, Principal::Anonymous))
            .then(|| Limits { attachment_bytes: core.attachment_limit(), import_bytes: core.import_limit() }),
    }
    .into())
}

/// What a Demo says. The demo Member's email and password are shown only
/// when an active Member who is not a Workspace Admin has that email
/// address, found as `sign_in` finds it, so nobody gets credentials that
/// fail or that hand them the Workspace.
fn demo_for_everyone(core: &Core, db: &Connection, settings: &crate::demo::DemoSettings) -> rusqlite::Result<Demo> {
    let workspace_admin: Option<bool> = db
        .query_row(
            "SELECT workspace_admin FROM members WHERE email = ?1 AND deactivated_at IS NULL",
            [normalize_email(&settings.email)],
            |row| row.get(0),
        )
        .optional()?;
    if workspace_admin == Some(true) {
        core.warn_demo_member_is_workspace_admin();
    }
    Ok(Demo {
        resets_every_hours: settings.resets_every_hours,
        account: (workspace_admin == Some(false)).then(|| Credentials {
            email: settings.email.clone(),
            password: settings.password.clone(),
        }),
    })
}

pub fn create_first_account(
    core: &Core,
    _: &Principal,
    input: NewAccount,
) -> OpResult<Member> {
    let name = valid_name(&input.name)?;
    let email = valid_email(&input.email)?;
    check_password(&input.password)?;
    let member = Member {
        id: random_hex(16)?,
        name,
        email,
        workspace_admin: true,
    };
    let password_hash = hash_password(&input.password)?;

    // Checked under the same lock as the insert, so two people completing
    // setup at the same moment cannot both become the first account.
    let mut db = core.db();
    let transaction = db.transaction()?;
    if any_member_exists(&transaction)? {
        return Err(OpError::Conflict(
            "This installation already has an account. Sign in instead.".into(),
        ));
    }
    transaction.execute(
        "INSERT INTO members (id, email, name, password_hash, workspace_admin, created_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5)",
        rusqlite::params![member.id, member.email, member.name, password_hash, core.now()],
    )?;
    super::areas::create_personal_area(&transaction, &member.id, core.now())?;
    let secret = start_session(core, &transaction, &member.id)?;
    transaction.commit()?;
    Ok(Reply {
        output: member,
        session: SessionChange::Start(secret),
    })
}

pub fn sign_in(core: &Core, _: &Principal, client: Client, input: Credentials) -> OpResult<Member> {
    let refused = || {
        OpError::NotPermitted("That email address and password do not match an account.".into())
    };
    let attempt = attempts::begin(core, client, Some(&input.email))?;
    // No account has a longer password, so it is refused without hashing it.
    if input.password.chars().count() > MAX_PASSWORD_LENGTH {
        return Err(refused());
    }
    let found = core.db().query_row(
        "SELECT id, name, email, workspace_admin, password_hash FROM members
         WHERE email = ?1 AND deactivated_at IS NULL",
        [normalize_email(&input.email)],
        |row| Ok((read_member(row)?, row.get::<_, String>(4)?)),
    );
    let (member, stored_hash) = match found {
        Ok(found) => (Some(found.0), found.1),
        // Check against a hash anyway, so the time taken does not reveal
        // which email addresses have an account.
        Err(rusqlite::Error::QueryReturnedNoRows) => (None, dummy_hash().to_string()),
        Err(error) => return Err(error.into()),
    };
    // Hashing is slow on purpose; it runs without holding the database.
    let matches = verify_password(&input.password, &stored_hash)?;
    let member = member.filter(|_| matches).ok_or_else(refused)?;
    attempt.succeeded();
    let secret = start_session(core, &core.db(), &member.id)?;
    Ok(Reply {
        output: member,
        session: SessionChange::Start(secret),
    })
}

pub fn sign_out(core: &Core, principal: &Principal, _: Nothing) -> OpResult<()> {
    if let Principal::Member {
        session: Some(session),
        ..
    } = principal
    {
        core.db()
            .execute("DELETE FROM sessions WHERE id = ?1", [&session.0])?;
        core.access_changed();
    }
    Ok(Reply {
        output: (),
        session: SessionChange::End,
    })
}

/// Finds out who holds a session secret. An unknown or expired secret is
/// simply nobody.
pub fn principal_for_session(core: &Core, secret: &str) -> Result<Principal, OpError> {
    let session = session_id(secret);
    let found = core.db().query_row(
        "SELECT members.id, members.workspace_admin
         FROM sessions JOIN members ON members.id = sessions.member_id
         WHERE sessions.id = ?1 AND sessions.expires_at > ?2
           AND members.deactivated_at IS NULL",
        rusqlite::params![session.0, core.now()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );
    match found {
        Ok((id, workspace_admin)) => Ok(Principal::Member {
            id,
            workspace_admin,
            session: Some(session),
            token: None,
        }),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Principal::Anonymous),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn valid_name(name: &str) -> Result<String, OpError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(OpError::InvalidInput("Enter a name.".into()));
    }
    Ok(name.to_string())
}

pub(super) fn valid_email(email: &str) -> Result<String, OpError> {
    let email = normalize_email(email);
    let well_formed = email
        .split_once('@')
        .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.') && !email.contains(char::is_whitespace));
    if !well_formed {
        return Err(OpError::InvalidInput("Enter a valid email address.".into()));
    }
    Ok(email)
}

pub(super) fn check_password(password: &str) -> Result<(), OpError> {
    let length = password.chars().count();
    if !(MIN_PASSWORD_LENGTH..=MAX_PASSWORD_LENGTH).contains(&length) {
        return Err(OpError::InvalidInput(format!(
            "Choose a password of {MIN_PASSWORD_LENGTH} to {MAX_PASSWORD_LENGTH} characters."
        )));
    }
    Ok(())
}

pub(super) fn read_member(row: &rusqlite::Row) -> rusqlite::Result<Member> {
    Ok(Member {
        id: row.get(0)?,
        name: row.get(1)?,
        email: row.get(2)?,
        workspace_admin: row.get(3)?,
    })
}

fn any_member_exists(db: &Connection) -> rusqlite::Result<bool> {
    db.query_row("SELECT EXISTS (SELECT 1 FROM members)", [], |row| row.get(0))
}

/// Starts a session for a Member and returns its secret. Only a hash of the
/// secret is stored, so reading the database does not let anyone sign in.
pub(super) fn start_session(core: &Core, db: &Connection, member_id: &str) -> Result<String, OpError> {
    let secret = random_hex(32)?;
    let now = core.now();
    db.execute(
        "INSERT INTO sessions (id, member_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![session_id(&secret).0, member_id, now, now + SESSION_LIFETIME],
    )?;
    Ok(secret)
}

fn session_id(secret: &str) -> SessionId {
    SessionId(digest(secret))
}

/// What is stored in place of a secret, so reading the database does not
/// reveal the secret itself.
pub(super) fn digest(secret: &str) -> String {
    hex(&Sha256::digest(secret.as_bytes()))
}

pub(super) fn hash_password(password: &str) -> Result<String, OpError> {
    // The default is Argon2id.
    let hash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|error| anyhow!("hashing a password failed: {error}"))?;
    Ok(hash.to_string())
}

/// Email addresses are compared without regard to case or surrounding spaces.
pub(super) fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

fn verify_password(password: &str, stored_hash: &str) -> Result<bool, OpError> {
    let hash = PasswordHash::new(stored_hash)
        .map_err(|error| anyhow!("a stored password hash is unreadable: {error}"))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &hash)
        .is_ok())
}

/// A hash no password matches, for signing in with an unknown email address.
fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| {
        let password = random_hex(32).unwrap_or_default();
        hash_password(&password).unwrap_or_default()
    })
}

pub(super) fn random_hex(bytes: usize) -> Result<String, OpError> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).map_err(|error| anyhow!("no randomness available: {error}"))?;
    Ok(hex(&buffer))
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::testing::{fresh, TestCore};

    fn ada() -> NewAccount {
        NewAccount {
            name: "Ada Lovelace".into(),
            email: "ada@example.org".into(),
            password: "correct horse battery".into(),
        }
    }

    /// The principal a browser would have after an operation started a session.
    fn signed_in<T>(core: &TestCore, reply: &Reply<T>) -> Principal {
        let SessionChange::Start(secret) = &reply.session else {
            panic!("no session was started");
        };
        principal_for_session(core, secret).unwrap()
    }

    #[test]
    fn the_first_person_creates_an_account_and_becomes_workspace_admin() {
        let core = fresh();
        let before = get_session(&core, &Principal::Anonymous, Nothing {}).unwrap();
        assert_eq!(
            before.output,
            Session {
                setup_required: true,
                sends_mail: false,
                member: None,
                agent: None,
                demo: None,
                release: None,
                limits: None,
            }
        );

        let created = create_first_account(&core, &Principal::Anonymous, ada()).unwrap();
        assert_eq!(created.output.name, "Ada Lovelace");
        assert_eq!(created.output.email, "ada@example.org");
        assert!(created.output.workspace_admin);

        let ada = signed_in(&core, &created);
        let after = get_session(&core, &ada, Nothing {}).unwrap().output;
        assert!(!after.setup_required);
        assert_eq!(after.member, Some(created.output));
        assert_eq!(after.agent, None);
        assert_eq!(after.demo, None);
    }

    #[test]
    fn someone_signed_in_learns_the_largest_file_and_zip_this_turtle_accepts() {
        let mut core = fresh();
        core.core.set_attachment_limit(5 * 1024 * 1024);
        core.core.set_import_limit(300 * 1024 * 1024);
        let created = create_first_account(&core, &Principal::Anonymous, ada()).unwrap();
        let ada = signed_in(&core, &created);

        let stranger = get_session(&core, &Principal::Anonymous, Nothing {}).unwrap().output;
        let signed_in = get_session(&core, &ada, Nothing {}).unwrap().output;

        assert_eq!(stranger.limits, None);
        assert_eq!(signed_in.limits, Some(Limits { attachment_bytes: 5_242_880, import_bytes: 314_572_800 }));
    }

    #[test]
    fn only_someone_signed_in_learns_which_release_this_is() {
        let core = fresh();
        let created = create_first_account(&core, &Principal::Anonymous, ada()).unwrap();
        let ada = signed_in(&core, &created);

        let stranger = get_session(&core, &Principal::Anonymous, Nothing {}).unwrap().output;
        assert_eq!(stranger.release, None);

        let release = get_session(&core, &ada, Nothing {}).unwrap().output.release.expect("no Release");
        // A test build is not a release build, so it names its commit and links no notes.
        assert!(release.number.starts_with(&format!("{}+", env!("CARGO_PKG_VERSION"))), "{release:?}");
        assert_eq!(release.notes, None);
    }

    #[test]
    fn setup_is_closed_once_an_account_exists() {
        let core = fresh();
        create_first_account(&core, &Principal::Anonymous, ada()).unwrap();

        let second = NewAccount {
            name: "Mallory".into(),
            email: "mallory@example.org".into(),
            password: "let me in please".into(),
        };
        let refused = create_first_account(&core, &Principal::Anonymous, second).unwrap_err();
        assert_eq!(refused.kind(), "conflict");

        let session = get_session(&core, &Principal::Anonymous, Nothing {}).unwrap();
        assert!(!session.output.setup_required);
    }

    #[test]
    fn a_member_signs_in_with_email_and_password_and_signs_out() {
        let core = fresh();
        let created = create_first_account(&core, &Principal::Anonymous, ada()).unwrap();

        let credentials = Credentials {
            email: "ada@example.org".into(),
            password: "correct horse battery".into(),
        };
        let signed = sign_in(&core, &Principal::Anonymous, None, credentials).unwrap();
        assert_eq!(signed.output, created.output);
        let SessionChange::Start(secret) = &signed.session else {
            panic!("signing in started no session");
        };
        let ada = principal_for_session(&core, secret).unwrap();
        let session = get_session(&core, &ada, Nothing {}).unwrap();
        assert_eq!(session.output.member, Some(created.output));

        let out = sign_out(&core, &ada, Nothing {}).unwrap();
        assert_eq!(out.session, SessionChange::End);
        assert_eq!(
            principal_for_session(&core, secret).unwrap(),
            Principal::Anonymous
        );
    }

    #[test]
    fn signing_out_ends_only_that_session() {
        let core = fresh();
        let first = create_first_account(&core, &Principal::Anonymous, ada()).unwrap();
        let credentials = Credentials {
            email: "ada@example.org".into(),
            password: "correct horse battery".into(),
        };
        let second = sign_in(&core, &Principal::Anonymous, None, credentials).unwrap();

        sign_out(&core, &signed_in(&core, &second), Nothing {}).unwrap();
        assert_ne!(signed_in(&core, &first), Principal::Anonymous);
    }

    #[test]
    fn a_wrong_password_or_unknown_email_is_refused_alike() {
        let core = fresh();
        create_first_account(&core, &Principal::Anonymous, ada()).unwrap();

        let wrong_password = Credentials {
            email: "ada@example.org".into(),
            password: "incorrect horse".into(),
        };
        let unknown_email = Credentials {
            email: "nobody@example.org".into(),
            password: "correct horse battery".into(),
        };
        let first = sign_in(&core, &Principal::Anonymous, None, wrong_password).unwrap_err();
        let second = sign_in(&core, &Principal::Anonymous, None, unknown_email).unwrap_err();
        assert_eq!(first.kind(), "not_permitted");
        // Nothing tells a caller which email addresses have an account.
        assert_eq!(first.kind(), second.kind());
        assert_eq!(first.message(), second.message());
    }

    fn from(address: &str) -> Option<std::net::IpAddr> {
        Some(address.parse().unwrap())
    }

    fn ada_with(password: &str) -> Credentials {
        Credentials {
            email: "ada@example.org".into(),
            password: password.into(),
        }
    }

    #[test]
    fn after_five_failed_sign_ins_with_an_email_address_it_is_refused_for_fifteen_minutes() {
        let core = fresh();
        create_first_account(&core, &Principal::Anonymous, ada()).unwrap();
        // From five different clients, so only the email address adds up.
        for client in ["198.51.100.1", "198.51.100.2", "198.51.100.3", "198.51.100.4", "198.51.100.5"] {
            let failed = sign_in(&core, &Principal::Anonymous, from(client), ada_with("incorrect horse")).unwrap_err();
            assert_eq!(failed.message(), "That email address and password do not match an account.");
        }

        let refused =
            sign_in(&core, &Principal::Anonymous, from("198.51.100.6"), ada_with("correct horse battery")).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
        assert_eq!(refused.message(), "Too many attempts failed. Try again in 15 minutes.");

        core.advance(14 * 60);
        let refused =
            sign_in(&core, &Principal::Anonymous, from("198.51.100.6"), ada_with("correct horse battery")).unwrap_err();
        assert_eq!(refused.message(), "Too many attempts failed. Try again in 1 minute.");

        core.advance(60);
        assert!(sign_in(&core, &Principal::Anonymous, from("198.51.100.6"), ada_with("correct horse battery")).is_ok());
    }

    #[test]
    fn after_twenty_failed_sign_ins_from_a_client_it_is_refused_whatever_the_email_address() {
        let core = fresh();
        create_first_account(&core, &Principal::Anonymous, ada()).unwrap();
        // Four tries each at five addresses, staying under the limit per address.
        for number in 0..20 {
            let guess = Credentials {
                email: format!("person{}@example.org", number % 5),
                password: "a guessed password".into(),
            };
            sign_in(&core, &Principal::Anonymous, from("203.0.113.7"), guess).unwrap_err();
        }

        let refused =
            sign_in(&core, &Principal::Anonymous, from("203.0.113.7"), ada_with("correct horse battery")).unwrap_err();
        assert_eq!(refused.message(), "Too many attempts failed. Try again in 15 minutes.");
        // Another client is not held up.
        assert!(sign_in(&core, &Principal::Anonymous, from("203.0.113.8"), ada_with("correct horse battery")).is_ok());
    }

    #[test]
    fn an_ipv6_client_is_counted_by_its_network() {
        let core = fresh();
        create_first_account(&core, &Principal::Anonymous, ada()).unwrap();
        for number in 0..20 {
            let guess = Credentials {
                email: format!("person{}@example.org", number % 5),
                password: "a guessed password".into(),
            };
            let client = from(&format!("2001:db8:1:2::{number:x}"));
            sign_in(&core, &Principal::Anonymous, client, guess).unwrap_err();
        }

        let same_network = from("2001:db8:1:2:ffff::1");
        let refused = sign_in(&core, &Principal::Anonymous, same_network, ada_with("correct horse battery")).unwrap_err();
        assert_eq!(refused.message(), "Too many attempts failed. Try again in 15 minutes.");
        let other_network = from("2001:db8:1:3::1");
        assert!(sign_in(&core, &Principal::Anonymous, other_network, ada_with("correct horse battery")).is_ok());
    }

    #[test]
    fn signing_in_too_often_with_an_unknown_email_address_is_refused_alike() {
        let core = fresh();
        create_first_account(&core, &Principal::Anonymous, ada()).unwrap();
        let nobody = || Credentials {
            email: "nobody@example.org".into(),
            password: "a guessed password".into(),
        };
        for _ in 0..5 {
            sign_in(&core, &Principal::Anonymous, None, ada_with("incorrect horse")).unwrap_err();
            sign_in(&core, &Principal::Anonymous, None, nobody()).unwrap_err();
        }

        let member = sign_in(&core, &Principal::Anonymous, None, ada_with("incorrect horse")).unwrap_err();
        let no_member = sign_in(&core, &Principal::Anonymous, None, nobody()).unwrap_err();
        assert_eq!(member.message(), "Too many attempts failed. Try again in 15 minutes.");
        assert_eq!((member.kind(), member.message()), (no_member.kind(), no_member.message()));
    }

    #[test]
    fn signing_in_successfully_does_not_count_as_a_failure() {
        let core = fresh();
        create_first_account(&core, &Principal::Anonymous, ada()).unwrap();
        for _ in 0..4 {
            sign_in(&core, &Principal::Anonymous, from("198.51.100.1"), ada_with("incorrect horse")).unwrap_err();
        }
        for _ in 0..20 {
            sign_in(&core, &Principal::Anonymous, from("198.51.100.1"), ada_with("correct horse battery")).unwrap();
        }

        // The fifth failure is still let through; only after it is Ada refused.
        let fifth = sign_in(&core, &Principal::Anonymous, from("198.51.100.1"), ada_with("incorrect horse")).unwrap_err();
        assert_eq!(fifth.message(), "That email address and password do not match an account.");
        let refused =
            sign_in(&core, &Principal::Anonymous, from("198.51.100.1"), ada_with("correct horse battery")).unwrap_err();
        assert_eq!(refused.message(), "Too many attempts failed. Try again in 15 minutes.");
    }

    #[test]
    fn a_refused_sign_in_does_not_hash_the_password() {
        let core = fresh();
        create_first_account(&core, &Principal::Anonymous, ada()).unwrap();
        let started = std::time::Instant::now();
        for _ in 0..5 {
            sign_in(&core, &Principal::Anonymous, None, ada_with("incorrect horse")).unwrap_err();
        }
        let one_hash = started.elapsed() / 5;

        let started = std::time::Instant::now();
        for _ in 0..5 {
            sign_in(&core, &Principal::Anonymous, None, ada_with("incorrect horse")).unwrap_err();
        }
        // Hashing takes long on purpose, far longer than turning someone away.
        assert!(started.elapsed() < one_hash, "five refusals took {:?}, one hash {one_hash:?}", started.elapsed());
    }

    #[test]
    fn a_session_expires() {
        let core = fresh();
        let created = create_first_account(&core, &Principal::Anonymous, ada()).unwrap();
        core.advance(SESSION_LIFETIME + 1);
        assert_eq!(signed_in(&core, &created), Principal::Anonymous);
    }

    #[test]
    fn an_account_needs_a_name_an_email_address_and_a_long_enough_password() {
        let core = fresh();
        let broken = [
            ("", "ada@example.org", "correct horse battery"),
            ("Ada", "not an address", "correct horse battery"),
            ("Ada", "ada@example.org", "short"),
        ];
        let too_long = "x".repeat(MAX_PASSWORD_LENGTH + 1);
        let broken = broken.into_iter().chain([("Ada", "ada@example.org", too_long.as_str())]);
        for (name, email, password) in broken {
            let input = NewAccount {
                name: name.into(),
                email: email.into(),
                password: password.into(),
            };
            let refused = create_first_account(&core, &Principal::Anonymous, input).unwrap_err();
            assert_eq!(refused.kind(), "invalid_input", "{name:?} {email:?} {password:?}");
        }
        assert!(get_session(&core, &Principal::Anonymous, Nothing {}).unwrap().output.setup_required);
    }

    #[test]
    fn the_email_address_is_matched_regardless_of_case_and_surrounding_spaces() {
        let core = fresh();
        let input = NewAccount {
            email: " Ada@Example.ORG ".into(),
            ..ada()
        };
        let created = create_first_account(&core, &Principal::Anonymous, input).unwrap();
        assert_eq!(created.output.email, "ada@example.org");

        let credentials = Credentials {
            email: "ADA@example.org".into(),
            password: "correct horse battery".into(),
        };
        assert!(sign_in(&core, &Principal::Anonymous, None, credentials).is_ok());
    }

    #[test]
    fn the_session_says_whether_turtle_sends_mail() {
        let without = fresh();
        let (with, _) = crate::ops::testing::fresh_with_mail(crate::mail::fake::FakeMailer::default());

        assert!(!get_session(&without, &Principal::Anonymous, Nothing {}).unwrap().output.sends_mail);
        assert!(get_session(&with, &Principal::Anonymous, Nothing {}).unwrap().output.sends_mail);
    }

    /// A fresh core told it is a Demo of `email` with the demo password.
    fn demo_of(email: &str, reset_hours: Option<&str>) -> TestCore {
        let mut core = fresh();
        let settings = crate::demo::from_settings(Some(email), Some("try turtle now"), reset_hours)
            .unwrap()
            .expect("demo mode is on");
        core.core.set_demo(Some(settings));
        core
    }

    #[test]
    fn without_demo_settings_the_session_says_nothing_about_a_demo() {
        let core = fresh();
        assert_eq!(get_session(&core, &Principal::Anonymous, Nothing {}).unwrap().output.demo, None);
    }

    #[test]
    fn a_demo_shows_the_account_of_a_member_who_is_not_workspace_admin() {
        let core = demo_of("member-grace@example.org", Some("6"));
        crate::ops::testing::member(&core, "Grace", false);

        let session = get_session(&core, &Principal::Anonymous, Nothing {}).unwrap().output;
        assert_eq!(
            session.demo,
            Some(Demo {
                resets_every_hours: Some(6),
                account: Some(Credentials {
                    email: "member-grace@example.org".into(),
                    password: "try turtle now".into(),
                }),
            })
        );
    }

    #[test]
    fn a_demo_without_an_interval_does_not_say_how_often_it_is_reset() {
        let core = demo_of("member-grace@example.org", None);
        crate::ops::testing::member(&core, "Grace", false);

        let demo = get_session(&core, &Principal::Anonymous, Nothing {}).unwrap().output.demo.unwrap();
        assert_eq!(demo.resets_every_hours, None);
        assert!(demo.account.is_some());
    }

    #[test]
    fn a_demo_finds_its_member_however_the_email_address_is_written() {
        let core = demo_of("  Member-Grace@Example.ORG ", Some("6"));
        crate::ops::testing::member(&core, "Grace", false);

        let demo = get_session(&core, &Principal::Anonymous, Nothing {}).unwrap().output.demo.unwrap();
        assert!(demo.account.is_some(), "{demo:?}");
    }

    #[test]
    fn a_demo_never_shows_the_account_of_a_workspace_admin() {
        let core = demo_of("member-ada@example.org", Some("6"));
        crate::ops::testing::member(&core, "Ada", true);

        let demo = get_session(&core, &Principal::Anonymous, Nothing {}).unwrap().output.demo.unwrap();
        assert_eq!(demo, Demo { resets_every_hours: Some(6), account: None });
    }

    #[test]
    fn a_demo_shows_no_account_until_a_member_has_its_email_address() {
        let core = demo_of("grace@example.org", Some("6"));

        let demo = get_session(&core, &Principal::Anonymous, Nothing {}).unwrap().output.demo.unwrap();
        assert_eq!(demo, Demo { resets_every_hours: Some(6), account: None });
    }

    #[test]
    fn a_demo_shows_no_account_of_a_deactivated_member() {
        let core = demo_of("member-grace@example.org", Some("6"));
        crate::ops::testing::member(&core, "Grace", false);
        let ada = crate::ops::testing::member(&core, "Ada", true);
        let grace = crate::ops::members::MemberId { id: "member-grace".into() };
        crate::ops::members::deactivate_member(&core, &ada, grace).unwrap();

        let demo = get_session(&core, &Principal::Anonymous, Nothing {}).unwrap().output.demo.unwrap();
        assert_eq!(demo.account, None);
    }

    #[test]
    fn a_demo_tells_the_same_to_everyone_signed_in_or_not() {
        let core = demo_of("member-grace@example.org", Some("6"));
        let grace = crate::ops::testing::member(&core, "Grace", false);
        let ada = crate::ops::testing::member(&core, "Ada", true);

        let anonymous = get_session(&core, &Principal::Anonymous, Nothing {}).unwrap().output.demo;
        assert!(anonymous.is_some());
        assert_eq!(get_session(&core, &grace, Nothing {}).unwrap().output.demo, anonymous);
        assert_eq!(get_session(&core, &ada, Nothing {}).unwrap().output.demo, anonymous);
    }
}
