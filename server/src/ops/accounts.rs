//! Accounts and sessions: the first account, signing in and signing out.

use super::{Catalogue, Core, OpError, OpResult, Principal, Reply, SessionChange, SessionId};
use anyhow::anyhow;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::Argon2;
use rusqlite::Connection;
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
        .add(
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

#[derive(Debug, Deserialize, JsonSchema)]
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
    }
    .into())
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

pub fn sign_in(core: &Core, _: &Principal, input: Credentials) -> OpResult<Member> {
    let refused = || {
        OpError::NotPermitted("That email address and password do not match an account.".into())
    };
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
            }
        );

        let created = create_first_account(&core, &Principal::Anonymous, ada()).unwrap();
        assert_eq!(created.output.name, "Ada Lovelace");
        assert_eq!(created.output.email, "ada@example.org");
        assert!(created.output.workspace_admin);

        let ada = signed_in(&core, &created);
        let after = get_session(&core, &ada, Nothing {}).unwrap();
        assert_eq!(
            after.output,
            Session {
                setup_required: false,
                sends_mail: false,
                member: Some(created.output),
                agent: None,
            }
        );
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
        let signed = sign_in(&core, &Principal::Anonymous, credentials).unwrap();
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
        let second = sign_in(&core, &Principal::Anonymous, credentials).unwrap();

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
        let first = sign_in(&core, &Principal::Anonymous, wrong_password).unwrap_err();
        let second = sign_in(&core, &Principal::Anonymous, unknown_email).unwrap_err();
        assert_eq!(first.kind(), "not_permitted");
        // Nothing tells a caller which email addresses have an account.
        assert_eq!(first.kind(), second.kind());
        assert_eq!(first.message(), second.message());
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
        assert!(sign_in(&core, &Principal::Anonymous, credentials).is_ok());
    }

    #[test]
    fn the_session_says_whether_turtle_sends_mail() {
        let without = fresh();
        let (with, _) = crate::ops::testing::fresh_with_mail(crate::mail::fake::FakeMailer::default());

        assert!(!get_session(&without, &Principal::Anonymous, Nothing {}).unwrap().output.sends_mail);
        assert!(get_session(&with, &Principal::Anonymous, Nothing {}).unwrap().output.sends_mail);
    }
}
