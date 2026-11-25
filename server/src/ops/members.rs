//! Members of the Workspace: inviting them, managing them, and resetting
//! their passwords through single-use links. Turtle mails the links when it
//! sends mail; otherwise a Workspace Admin passes them on.

use super::accounts::{
    check_password, digest, hash_password, random_hex, read_member, start_session, valid_email, valid_name,
    Member, Nothing,
};
use super::{profiles, Catalogue, Core, OpError, OpResult, Principal, Reply, SessionChange};
use crate::mail::Mail;
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue.add(
        "invite_member",
        "Invites a person by email address with a single-use link that expires: Turtle mails it to them when it sends mail, and otherwise returns it to pass on. Only for a Workspace Admin.",
        invite_member,
    );
    catalogue
        .add(
            "accept_invitation",
            "Accepts an invitation: the invited person sets name and password, becomes a Member and is signed in.",
            accept_invitation,
        )
        .browser_only = true;
    catalogue
        .add(
            "list_members",
            "Lists every Member by name, deactivated ones included. Only for a Workspace Admin.",
            list_members,
        )
        .changes = false;
    catalogue.add(
        "deactivate_member",
        "Deactivates a Member: their sessions end at once and they can no longer sign in. Only for a Workspace Admin.",
        deactivate_member,
    );
    catalogue.add(
        "set_workspace_admin",
        "Makes a Member Workspace Admin, or takes that away. The last Workspace Admin keeps it. Only for a Workspace Admin.",
        set_workspace_admin,
    );
    catalogue.add(
        "create_password_reset",
        "Creates a single-use link that lets a Member set a new password; it expires. Turtle mails it to them when it sends mail, and otherwise returns it to pass on. Only for a Workspace Admin.",
        create_password_reset,
    );
    catalogue
        .add(
            "request_password_reset",
            "Mails a password reset link to the Member with this email address, if there is one. The answer is the same either way. Only when Turtle sends mail.",
            request_password_reset,
        )
        .browser_only = true;
    catalogue
        .add(
            "reset_password",
            "Sets a new password through a reset link, ends the Member's other sessions and signs them in.",
            reset_password,
        )
        .browser_only = true;
}

/// How long an invitation link can be used.
pub const INVITATION_LIFETIME: i64 = 7 * 24 * 60 * 60;

/// How long a password reset link can be used.
pub const PASSWORD_RESET_LIFETIME: i64 = 24 * 60 * 60;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewPassword {
    /// The secret at the end of the reset link.
    pub secret: String,
    pub password: String,
}

/// How long after a reset link was mailed another is not, so asking again
/// and again does not flood a Member's mailbox.
pub const RESET_REQUEST_INTERVAL: i64 = 5 * 60;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResetRequest {
    /// The email address of the account.
    pub email: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Invite {
    pub email: String,
}

/// A single-use link: mailed to the person it is for when Turtle sends mail,
/// and otherwise returned for a Workspace Admin to pass on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SingleUseLink {
    /// The address of the link, starting with a slash; it goes after the
    /// address Turtle is reached at. Absent when the link was mailed, so
    /// only the person it is for holds it.
    pub address: Option<String>,
    /// The email address the link was mailed to; absent when it is to be
    /// passed on.
    pub mailed_to: Option<String>,
    /// When the link stops working, in seconds since the Unix epoch.
    pub expires_at: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Joining {
    /// The secret at the end of the invitation link.
    pub secret: String,
    pub name: String,
    pub password: String,
}

/// A Member as a Workspace Admin sees them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct MemberEntry {
    pub id: String,
    pub name: String,
    pub email: String,
    pub workspace_admin: bool,
    /// False once the Member has been deactivated.
    pub active: bool,
    /// The address their Profile picture is shown from; absent without one.
    pub picture: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct MemberList {
    pub members: Vec<MemberEntry>,
}

pub fn list_members(core: &Core, principal: &Principal, _: Nothing) -> OpResult<MemberList> {
    require_workspace_admin(principal)?;
    let db = core.db();
    let mut statement = db.prepare(&format!(
        "SELECT {ENTRY_COLUMNS} FROM members ORDER BY name COLLATE NOCASE, id"
    ))?;
    let members = statement
        .query_map([], read_entry)?
        .collect::<Result<_, _>>()?;
    Ok(MemberList { members }.into())
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemberId {
    pub id: String,
}

pub fn deactivate_member(core: &Core, principal: &Principal, input: MemberId) -> OpResult<MemberEntry> {
    require_workspace_admin(principal)?;
    let mut db = core.db();
    let transaction = db.transaction()?;
    let target = member_entry(&transaction, &input.id)?;
    if target.workspace_admin && target.active {
        keep_one_workspace_admin(&transaction, &target.id)?;
    }
    if target.active {
        transaction.execute(
            "UPDATE members SET deactivated_at = ?1 WHERE id = ?2",
            rusqlite::params![core.now(), target.id],
        )?;
        transaction.execute("DELETE FROM sessions WHERE member_id = ?1", [&target.id])?;
        transaction.execute("DELETE FROM single_use_links WHERE member_id = ?1", [&target.id])?;
    }
    transaction.commit()?;
    core.access_changed();
    Ok(MemberEntry {
        active: false,
        ..target
    }
    .into())
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetWorkspaceAdmin {
    /// The Member's ID.
    pub id: String,
    /// Whether they are to be Workspace Admin.
    pub workspace_admin: bool,
}

pub fn set_workspace_admin(
    core: &Core,
    principal: &Principal,
    input: SetWorkspaceAdmin,
) -> OpResult<MemberEntry> {
    require_workspace_admin(principal)?;
    let mut db = core.db();
    let transaction = db.transaction()?;
    let target = member_entry(&transaction, &input.id)?;
    if input.workspace_admin && !target.active {
        return Err(OpError::Conflict(
            "A deactivated Member cannot be made Workspace Admin.".into(),
        ));
    }
    if !input.workspace_admin && target.workspace_admin && target.active {
        keep_one_workspace_admin(&transaction, &target.id)?;
    }
    transaction.execute(
        "UPDATE members SET workspace_admin = ?1 WHERE id = ?2",
        rusqlite::params![input.workspace_admin, target.id],
    )?;
    transaction.commit()?;
    core.access_changed();
    Ok(MemberEntry {
        workspace_admin: input.workspace_admin,
        ..target
    }
    .into())
}

pub fn invite_member(core: &Core, principal: &Principal, input: Invite) -> OpResult<SingleUseLink> {
    require_workspace_admin(principal)?;
    let email = valid_email(&input.email)?;
    let Principal::Member { id: inviter, .. } = principal else {
        unreachable!("only a Member is Workspace Admin")
    };
    let db = core.db();
    let taken: bool = db.query_row(
        "SELECT EXISTS (SELECT 1 FROM members WHERE email = ?1)",
        [&email],
        |row| row.get(0),
    )?;
    if taken {
        return Err(OpError::Conflict(
            "There already is a Member with this email address.".into(),
        ));
    }
    let issued = issue_link(core, &db, LinkFor::Invitation(&email))?;
    let inviter: String = db.query_row("SELECT name FROM members WHERE id = ?1", [inviter], |row| row.get(0))?;
    drop(db);
    let mail = |link: &str| Mail {
        to: email.clone(),
        subject: format!("{inviter} invites you to Turtle"),
        body: format!(
            "{inviter} invites you to join their Workspace on Turtle.\n\n\
             Choose your name and password here:\n{link}\n\n\
             The link works once, within seven days.\n"
        ),
    };
    Ok(deliver(core, issued, mail)?.into())
}

pub fn accept_invitation(core: &Core, _: &Principal, input: Joining) -> OpResult<Member> {
    let name = valid_name(&input.name)?;
    check_password(&input.password)?;
    // Hashing is slow on purpose; it runs without holding the database.
    let password_hash = hash_password(&input.password)?;
    let id = random_hex(16)?;

    let mut db = core.db();
    let transaction = db.transaction()?;
    let email: String = transaction
        .query_row(
            "SELECT email FROM single_use_links WHERE id = ?1 AND kind = 'invitation' AND expires_at > ?2",
            rusqlite::params![digest(&input.secret), core.now()],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(unusable_link)?;
    transaction.execute("DELETE FROM single_use_links WHERE id = ?1", [digest(&input.secret)])?;
    let inserted = transaction.execute(
        "INSERT INTO members (id, email, name, password_hash, workspace_admin, created_at)
         VALUES (?1, ?2, ?3, ?4, 0, ?5)
         ON CONFLICT (email) DO NOTHING",
        rusqlite::params![id, email, name, password_hash, core.now()],
    )?;
    if inserted == 0 {
        return Err(OpError::Conflict(
            "There already is a Member with this email address. Sign in instead.".into(),
        ));
    }
    super::areas::create_personal_area(&transaction, &id, core.now())?;
    let secret = start_session(core, &transaction, &id)?;
    transaction.commit()?;
    Ok(Reply {
        output: Member {
            id,
            name,
            email,
            workspace_admin: false,
        },
        session: SessionChange::Start(secret),
    })
}

const ENTRY_COLUMNS: &str = "id, name, email, workspace_admin, deactivated_at IS NULL, picture_hash";

fn read_entry(row: &rusqlite::Row) -> rusqlite::Result<MemberEntry> {
    let id: String = row.get(0)?;
    let picture_hash: Option<String> = row.get(5)?;
    Ok(MemberEntry {
        picture: profiles::picture_address(&id, picture_hash.as_deref()),
        id,
        name: row.get(1)?,
        email: row.get(2)?,
        workspace_admin: row.get(3)?,
        active: row.get(4)?,
    })
}

fn member_entry(db: &Connection, id: &str) -> Result<MemberEntry, OpError> {
    db.query_row(
        &format!("SELECT {ENTRY_COLUMNS} FROM members WHERE id = ?1"),
        [id],
        read_entry,
    )
    .optional()?
    .ok_or_else(|| OpError::NotFound("There is no such Member.".into()))
}

/// Refuses to let a Workspace Admin go if no other active one would remain,
/// so somebody can always manage the Members.
fn keep_one_workspace_admin(db: &Connection, leaving: &str) -> Result<(), OpError> {
    let others: bool = db.query_row(
        "SELECT EXISTS (SELECT 1 FROM members
         WHERE workspace_admin = 1 AND deactivated_at IS NULL AND id != ?1)",
        [leaving],
        |row| row.get(0),
    )?;
    if others {
        Ok(())
    } else {
        Err(OpError::Conflict(
            "This is the last Workspace Admin. Make another Member Workspace Admin first.".into(),
        ))
    }
}

pub fn create_password_reset(core: &Core, principal: &Principal, input: MemberId) -> OpResult<SingleUseLink> {
    require_workspace_admin(principal)?;
    let db = core.db();
    let target = member_entry(&db, &input.id)?;
    if !target.active {
        return Err(OpError::Conflict(
            "A deactivated Member cannot sign in, so their password cannot be reset.".into(),
        ));
    }
    let issued = issue_link(core, &db, LinkFor::PasswordReset(&target.id))?;
    drop(db);
    Ok(deliver(core, issued, |link| reset_mail(&target.email, link))?.into())
}

pub fn request_password_reset(core: &Core, _: &Principal, input: ResetRequest) -> OpResult<()> {
    let Some(mailer) = core.mailer().cloned() else {
        return Err(OpError::NotPermitted(
            "Turtle does not send mail here. Ask a Workspace Admin for a password reset link.".into(),
        ));
    };
    let email = valid_email(&input.email)?;
    // Nothing in the answer, nor in how long it takes, tells the caller
    // whether the address has an account: every address takes the same
    // statement, which stores a link only for an active Member not mailed
    // one lately, and the mail goes out in the background.
    let secret = random_hex(32)?;
    let now = core.now();
    let stored = core.db().execute(
        "INSERT INTO single_use_links (id, kind, email, member_id, created_at, expires_at)
         SELECT ?1, 'password_reset', NULL, members.id, ?3, ?4 FROM members
         WHERE members.email = ?2 AND members.deactivated_at IS NULL
           AND NOT EXISTS (SELECT 1 FROM single_use_links
                           WHERE kind = 'password_reset' AND member_id = members.id AND created_at > ?5)",
        rusqlite::params![digest(&secret), email, now, now + PASSWORD_RESET_LIFETIME, now - RESET_REQUEST_INTERVAL],
    )?;
    let link = full_address(core, &format!("/{RESET_PATH}/{secret}"))?;
    let mail = (stored == 1).then(|| reset_mail(&email, &link));
    std::thread::spawn(move || {
        if let Err(error) = mail.map_or(Ok(()), |mail| mailer.send(&mail)) {
            eprintln!("mailing a password reset link failed: {error:#}");
        }
    });
    Ok(().into())
}

/// The mail that brings a Member a password reset link.
fn reset_mail(to: &str, link: &str) -> Mail {
    Mail {
        to: to.to_string(),
        subject: "Set a new password for Turtle".into(),
        body: format!(
            "A new password was requested for your account on Turtle.\n\n\
             Choose it here:\n{link}\n\n\
             The link works once, within 24 hours. If you did not ask for it, \
             ignore this mail: your password stays as it is.\n"
        ),
    }
}

pub fn reset_password(core: &Core, _: &Principal, input: NewPassword) -> OpResult<Member> {
    check_password(&input.password)?;
    // Hashing is slow on purpose; it runs without holding the database.
    let password_hash = hash_password(&input.password)?;

    let mut db = core.db();
    let transaction = db.transaction()?;
    let member = transaction
        .query_row(
            "SELECT members.id, members.name, members.email, members.workspace_admin
             FROM single_use_links JOIN members ON members.id = single_use_links.member_id
             WHERE single_use_links.id = ?1 AND single_use_links.kind = 'password_reset' AND single_use_links.expires_at > ?2
               AND members.deactivated_at IS NULL",
            rusqlite::params![digest(&input.secret), core.now()],
            read_member,
        )
        .optional()?
        .ok_or_else(unusable_link)?;
    // This link and any other reset link for the Member stop working.
    transaction.execute(
        "DELETE FROM single_use_links WHERE kind = 'password_reset' AND member_id = ?1",
        [&member.id],
    )?;
    transaction.execute(
        "UPDATE members SET password_hash = ?1 WHERE id = ?2",
        rusqlite::params![password_hash, member.id],
    )?;
    // Whoever is signed in with the old password is signed out.
    transaction.execute("DELETE FROM sessions WHERE member_id = ?1", [&member.id])?;
    let secret = start_session(core, &transaction, &member.id)?;
    transaction.commit()?;
    core.access_changed();
    Ok(Reply {
        output: member,
        session: SessionChange::Start(secret),
    })
}

/// Who a single-use link is for.
enum LinkFor<'a> {
    /// The person with this email address, to join as a Member.
    Invitation(&'a str),
    /// The Member with this ID, to set a new password.
    PasswordReset(&'a str),
}

/// A single-use link just stored, before it is mailed or returned.
struct IssuedLink {
    /// Its address after the public URL, starting with a slash.
    address: String,
    /// What the database keeps in place of its secret.
    id: String,
    expires_at: i64,
}

/// Where a password reset link leads, after the public URL.
const RESET_PATH: &str = "reset-password";

/// The whole address of a link, for a mail.
fn full_address(core: &Core, address: &str) -> Result<String, OpError> {
    let public_url = core
        .public_url()
        .ok_or_else(|| anyhow::anyhow!("Turtle sends mail but has no public URL to link to"))?;
    Ok(format!("{public_url}{address}"))
}

/// Mails a link to the person it is for when Turtle sends mail, or else
/// returns it to pass on. A link whose mail could not be sent is removed, as
/// nobody holds it.
fn deliver(core: &Core, issued: IssuedLink, mail: impl FnOnce(&str) -> Mail) -> Result<SingleUseLink, OpError> {
    let Some(mailer) = core.mailer() else {
        return Ok(SingleUseLink {
            address: Some(issued.address),
            mailed_to: None,
            expires_at: issued.expires_at,
        });
    };
    let sent = full_address(core, &issued.address).and_then(|link| {
        let mail = mail(&link);
        mailer.send(&mail)?;
        Ok(mail.to)
    });
    let to = match sent {
        Ok(to) => to,
        Err(error) => {
            core.db().execute("DELETE FROM single_use_links WHERE id = ?1", [&issued.id])?;
            return Err(error);
        }
    };
    Ok(SingleUseLink {
        address: None,
        mailed_to: Some(to),
        expires_at: issued.expires_at,
    })
}

/// Stores a new single-use link. Only a digest of its secret is kept, so
/// reading the database does not let anyone use it.
fn issue_link(core: &Core, db: &Connection, about: LinkFor) -> Result<IssuedLink, OpError> {
    let secret = random_hex(32)?;
    let now = core.now();
    let (kind, email, member_id, lifetime, path) = match about {
        LinkFor::Invitation(email) => ("invitation", Some(email), None, INVITATION_LIFETIME, "join"),
        LinkFor::PasswordReset(id) => ("password_reset", None, Some(id), PASSWORD_RESET_LIFETIME, RESET_PATH),
    };
    let expires_at = now + lifetime;
    let id = digest(&secret);
    db.execute(
        "INSERT INTO single_use_links (id, kind, email, member_id, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, kind, email, member_id, now, expires_at],
    )?;
    Ok(IssuedLink {
        address: format!("/{path}/{secret}"),
        id,
        expires_at,
    })
}

fn unusable_link() -> OpError {
    OpError::NotPermitted(
        "This link has already been used or has expired. Ask a Workspace Admin for a new one.".into(),
    )
}

/// Only a Workspace Admin manages Members.
fn require_workspace_admin(principal: &Principal) -> Result<(), OpError> {
    super::require_workspace_admin(principal, "Only a Workspace Admin can manage Members.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::accounts::{get_session, principal_for_session, sign_in, Credentials};
    use crate::ops::profiles::{set_profile_picture, NewProfilePicture};
    use crate::mail::fake::FakeMailer;
    use crate::mail::Mail;
    use crate::ops::testing::{fresh, fresh_with_mail, member};
    use crate::ops::{Reply, SessionChange};

    /// The secret at the end of a link's address.
    fn secret_of(link: &SingleUseLink) -> String {
        let address = link.address.as_deref().expect("the link was returned, not mailed");
        address.rsplit('/').next().unwrap().to_string()
    }

    /// The link a mail holds, as a link a Workspace Admin would get.
    fn link_in(mail: &Mail) -> SingleUseLink {
        let link = mail
            .body
            .split_whitespace()
            .find(|word| word.starts_with("https://turtle.example.org/"))
            .expect("the mail holds a link to Turtle");
        SingleUseLink {
            address: Some(link.trim_start_matches("https://turtle.example.org").to_string()),
            mailed_to: None,
            expires_at: 0,
        }
    }

    fn signed_in<T>(core: &Core, reply: &Reply<T>) -> Principal {
        let SessionChange::Start(secret) = &reply.session else {
            panic!("no session was started");
        };
        principal_for_session(core, secret).unwrap()
    }

    fn joining(link: &SingleUseLink, name: &str) -> Joining {
        Joining {
            secret: secret_of(link),
            name: name.into(),
            password: "a long enough password".into(),
        }
    }

    #[test]
    fn an_invited_person_sets_name_and_password_and_is_signed_in_as_a_member() {
        let core = fresh();
        let admin = member(&core, "Ada", true);

        let link = invite_member(&core, &admin, Invite { email: "Grace@Example.org".into() })
            .unwrap()
            .output;
        assert!(secret_of(&link).len() > 16);
        assert!(link.address.as_deref().unwrap().starts_with("/join/"), "{link:?}");
        assert_eq!(link.mailed_to, None);

        let joined = accept_invitation(&core, &Principal::Anonymous, joining(&link, "Grace Hopper")).unwrap();
        assert_eq!(joined.output.name, "Grace Hopper");
        assert_eq!(joined.output.email, "grace@example.org");
        assert!(!joined.output.workspace_admin);
        let grace = signed_in(&core, &joined);
        let session = get_session(&core, &grace, Nothing {}).unwrap();
        assert_eq!(session.output.member, Some(joined.output));
    }

    #[test]
    fn an_invitation_link_works_once_and_not_after_it_expires() {
        let core = fresh();
        let admin = member(&core, "Ada", true);
        let invite = |email: &str| {
            invite_member(&core, &admin, Invite { email: email.into() }).unwrap().output
        };

        let used = invite("grace@example.org");
        accept_invitation(&core, &Principal::Anonymous, joining(&used, "Grace")).unwrap();
        let again = accept_invitation(&core, &Principal::Anonymous, joining(&used, "Mallory")).unwrap_err();
        assert_eq!(again.kind(), "not_permitted");

        let late = invite("linus@example.org");
        core.advance(INVITATION_LIFETIME + 1);
        let expired = accept_invitation(&core, &Principal::Anonymous, joining(&late, "Linus")).unwrap_err();
        assert_eq!(expired.kind(), "not_permitted");

        let made_up = SingleUseLink { address: Some("/join/0123".into()), mailed_to: None, expires_at: 0 };
        let unknown = accept_invitation(&core, &Principal::Anonymous, joining(&made_up, "Eve")).unwrap_err();
        assert_eq!(unknown.kind(), "not_permitted");
    }

    fn entry(id: &str, name: &str, workspace_admin: bool, active: bool) -> MemberEntry {
        MemberEntry {
            id: id.into(),
            name: name.into(),
            email: format!("{id}@example.org"),
            workspace_admin,
            active,
            picture: None,
        }
    }

    #[test]
    fn the_members_list_shows_each_members_picture() {
        let core = fresh();
        let admin = member(&core, "Ada", true);
        let ken = member(&core, "Ken", false);
        // A PNG of one green pixel.
        let content = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
        let set = set_profile_picture(&core, &ken, NewProfilePicture { content: content.into() }).unwrap();

        let list = list_members(&core, &admin, Nothing {}).unwrap().output;
        let pictures: Vec<_> = list.members.into_iter().map(|entry| entry.picture).collect();
        assert_eq!(pictures, [None, set.output.picture]);
    }

    #[test]
    fn a_workspace_admin_lists_the_members_by_name() {
        let core = fresh();
        let admin = member(&core, "Ada", true);
        member(&core, "Charles", false);
        member(&core, "Barbara", false);

        let list = list_members(&core, &admin, Nothing {}).unwrap().output;
        assert_eq!(
            list.members,
            [
                entry("member-ada", "Ada", true, true),
                entry("member-barbara", "Barbara", false, true),
                entry("member-charles", "Charles", false, true),
            ]
        );
    }

    #[test]
    fn a_member_who_joins_gets_a_personal_area_that_outlasts_deactivation() {
        use crate::ops::areas::{get_area, AreaId};
        use crate::ops::tree::{create_page, get_tree, NewEntry, TreeOfArea};
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let joined = grace_joins(&core, &ada);
        let grace = signed_in(&core, &joined);
        let personal = crate::ops::testing::personal_area(&core, &grace);
        let note = NewEntry { area_id: personal.clone(), parent_id: None, title: "Ideas".into() };
        create_page(&core, &grace, note.into()).unwrap();

        deactivate_member(&core, &ada, MemberId { id: joined.output.id.clone() }).unwrap();
        let hidden = get_area(&core, &ada, AreaId { id: personal.clone() }).unwrap_err();
        assert_eq!(hidden.kind(), "not_found");

        // Nothing reactivates a Member yet; this is what it will undo.
        core.db().execute("UPDATE members SET deactivated_at = NULL WHERE id = ?1", [&joined.output.id]).unwrap();
        let back = grace_signs_in(&core).unwrap();
        let grace = signed_in(&core, &back);
        assert_eq!(crate::ops::testing::personal_area(&core, &grace), personal);
        let tree = get_tree(&core, &grace, TreeOfArea { area_id: personal }).unwrap().output;
        let titles: Vec<_> = tree.entries.iter().map(|entry| entry.title.as_str()).collect();
        assert_eq!(titles, ["Ideas"]);
    }

    /// Invites Grace and lets her join; returns the reply that signed her in.
    fn grace_joins(core: &Core, admin: &Principal) -> Reply<Member> {
        let link = invite_member(core, admin, Invite { email: "grace@example.org".into() })
            .unwrap()
            .output;
        accept_invitation(core, &Principal::Anonymous, joining(&link, "Grace")).unwrap()
    }

    fn grace_signs_in(core: &Core) -> Result<Reply<Member>, OpError> {
        let credentials = Credentials {
            email: "grace@example.org".into(),
            password: "a long enough password".into(),
        };
        sign_in(core, &Principal::Anonymous, credentials)
    }

    #[test]
    fn a_deactivated_member_is_signed_out_at_once_and_cannot_sign_in_again() {
        let core = fresh();
        let admin = member(&core, "Ada", true);
        let joined = grace_joins(&core, &admin);
        let other_session = grace_signs_in(&core).unwrap();

        deactivate_member(&core, &admin, MemberId { id: joined.output.id.clone() }).unwrap();

        assert_eq!(signed_in(&core, &joined), Principal::Anonymous);
        assert_eq!(signed_in(&core, &other_session), Principal::Anonymous);
        assert_eq!(grace_signs_in(&core).unwrap_err().kind(), "not_permitted");
        let list = list_members(&core, &admin, Nothing {}).unwrap().output;
        let grace = list.members.iter().find(|entry| entry.id == joined.output.id).unwrap();
        assert!(!grace.active);
    }

    fn admin_change(id: &str, workspace_admin: bool) -> SetWorkspaceAdmin {
        SetWorkspaceAdmin { id: id.into(), workspace_admin }
    }

    #[test]
    fn a_promoted_member_is_workspace_admin_at_once_and_can_be_demoted_again() {
        let core = fresh();
        let admin = member(&core, "Ada", true);
        let joined = grace_joins(&core, &admin);
        let id = joined.output.id.clone();

        let promoted = set_workspace_admin(&core, &admin, admin_change(&id, true)).unwrap();
        assert!(promoted.output.workspace_admin);
        // Her open session acts with the new standing, without signing in again.
        let grace = signed_in(&core, &joined);
        assert!(list_members(&core, &grace, Nothing {}).is_ok());

        set_workspace_admin(&core, &admin, admin_change(&id, false)).unwrap();
        let refused = list_members(&core, &signed_in(&core, &joined), Nothing {}).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
    }

    #[test]
    fn the_last_workspace_admin_can_be_neither_deactivated_nor_demoted() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        member(&core, "Barbara", true);

        deactivate_member(&core, &ada, MemberId { id: "member-barbara".into() }).unwrap();
        let deactivated = deactivate_member(&core, &ada, MemberId { id: "member-ada".into() }).unwrap_err();
        let demoted = set_workspace_admin(&core, &ada, admin_change("member-ada", false)).unwrap_err();
        assert_eq!(deactivated.kind(), "conflict");
        assert_eq!(demoted.kind(), "conflict");
        let list = list_members(&core, &ada, Nothing {}).unwrap().output;
        assert_eq!(list.members[0], entry("member-ada", "Ada", true, true));
    }

    #[test]
    fn a_deactivated_member_cannot_be_promoted() {
        let core = fresh();
        let admin = member(&core, "Ada", true);
        member(&core, "Barbara", false);
        deactivate_member(&core, &admin, MemberId { id: "member-barbara".into() }).unwrap();

        let refused = set_workspace_admin(&core, &admin, admin_change("member-barbara", true)).unwrap_err();
        assert_eq!(refused.kind(), "conflict");
    }

    fn new_password(link: &SingleUseLink) -> NewPassword {
        NewPassword {
            secret: secret_of(link),
            password: "an even better password".into(),
        }
    }

    #[test]
    fn a_member_who_forgot_their_password_sets_a_new_one_through_a_reset_link() {
        let core = fresh();
        let admin = member(&core, "Ada", true);
        let joined = grace_joins(&core, &admin);

        let link = create_password_reset(&core, &admin, MemberId { id: joined.output.id.clone() })
            .unwrap()
            .output;
        assert!(link.address.as_deref().unwrap().starts_with("/reset-password/"), "{link:?}");
        let reset = reset_password(&core, &Principal::Anonymous, new_password(&link)).unwrap();

        assert_eq!(reset.output, joined.output);
        assert_ne!(signed_in(&core, &reset), Principal::Anonymous);
        // Whoever knew the old password is signed out.
        assert_eq!(signed_in(&core, &joined), Principal::Anonymous);
        assert_eq!(grace_signs_in(&core).unwrap_err().kind(), "not_permitted");
        let credentials = Credentials {
            email: "grace@example.org".into(),
            password: "an even better password".into(),
        };
        assert!(sign_in(&core, &Principal::Anonymous, credentials).is_ok());
    }

    #[test]
    fn a_reset_link_works_once_and_not_after_it_expires() {
        let core = fresh();
        let admin = member(&core, "Ada", true);
        let joined = grace_joins(&core, &admin);
        let reset_link = || {
            create_password_reset(&core, &admin, MemberId { id: joined.output.id.clone() })
                .unwrap()
                .output
        };

        let used = reset_link();
        reset_password(&core, &Principal::Anonymous, new_password(&used)).unwrap();
        let again = reset_password(&core, &Principal::Anonymous, new_password(&used)).unwrap_err();
        assert_eq!(again.kind(), "not_permitted");

        let late = reset_link();
        core.advance(PASSWORD_RESET_LIFETIME + 1);
        let expired = reset_password(&core, &Principal::Anonymous, new_password(&late)).unwrap_err();
        assert_eq!(expired.kind(), "not_permitted");
    }

    #[test]
    fn an_invitation_link_does_not_reset_a_password() {
        let core = fresh();
        let admin = member(&core, "Ada", true);
        let invitation = invite_member(&core, &admin, Invite { email: "grace@example.org".into() })
            .unwrap()
            .output;
        let refused = reset_password(&core, &Principal::Anonymous, new_password(&invitation)).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
    }

    #[test]
    fn members_who_are_not_workspace_admin_cannot_manage_members() {
        let core = fresh();
        let admin = member(&core, "Ada", true);
        let barbara = member(&core, "Barbara", false);
        let catalogue = crate::ops::catalogue();
        let attempts = [
            ("invite_member", serde_json::json!({ "email": "eve@example.org" })),
            ("list_members", serde_json::json!({})),
            ("deactivate_member", serde_json::json!({ "id": "member-ada" })),
            ("set_workspace_admin", serde_json::json!({ "id": "member-barbara", "workspace_admin": true })),
            ("create_password_reset", serde_json::json!({ "id": "member-ada" })),
        ];
        for caller in [&barbara, &Principal::Anonymous] {
            for (operation, input) in &attempts {
                let refused = catalogue.call(&core, caller, operation, input.clone()).unwrap_err();
                assert_eq!(refused.kind(), "not_permitted", "{operation} as {caller:?}");
            }
        }
        let list = list_members(&core, &admin, Nothing {}).unwrap().output;
        assert_eq!(
            list.members,
            [entry("member-ada", "Ada", true, true), entry("member-barbara", "Barbara", false, true)]
        );
    }

    #[test]
    fn a_member_cannot_be_invited_twice() {
        let core = fresh();
        let admin = member(&core, "Ada", true);
        let refused = invite_member(&core, &admin, Invite { email: "Member-Ada@example.org".into() }).unwrap_err();
        assert_eq!(refused.kind(), "conflict");
    }

    #[test]
    fn setting_a_new_password_cancels_the_other_reset_links() {
        let core = fresh();
        let admin = member(&core, "Ada", true);
        let joined = grace_joins(&core, &admin);
        let id = MemberId { id: joined.output.id.clone() };
        let first = create_password_reset(&core, &admin, MemberId { id: id.id.clone() }).unwrap().output;
        let second = create_password_reset(&core, &admin, id).unwrap().output;

        reset_password(&core, &Principal::Anonymous, new_password(&second)).unwrap();
        let refused = reset_password(&core, &Principal::Anonymous, new_password(&first)).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
    }

    #[test]
    fn with_mail_an_invitation_is_mailed_to_the_invited_address_and_not_returned() {
        let (core, mailer) = fresh_with_mail(FakeMailer::default());
        let admin = member(&core, "Ada", true);

        let invited = invite_member(&core, &admin, Invite { email: "Grace@Example.org".into() })
            .unwrap()
            .output;
        assert_eq!(invited.address, None);
        assert_eq!(invited.mailed_to.as_deref(), Some("grace@example.org"));

        let sent = mailer.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to, "grace@example.org");
        assert!(sent[0].body.contains("Ada"), "the mail says who invites: {}", sent[0].body);
        let link = link_in(&sent[0]);
        assert!(link.address.as_deref().unwrap().starts_with("/join/"), "{link:?}");
        let joined = accept_invitation(&core, &Principal::Anonymous, joining(&link, "Grace")).unwrap();
        assert_eq!(joined.output.email, "grace@example.org");
    }

    #[test]
    fn with_mail_a_password_reset_a_workspace_admin_creates_is_mailed_to_the_member() {
        let (core, mailer) = fresh_with_mail(FakeMailer::default());
        let admin = member(&core, "Ada", true);
        let ken = member(&core, "Ken", false);
        let Principal::Member { id, .. } = &ken else { unreachable!() };

        let reset = create_password_reset(&core, &admin, MemberId { id: id.clone() }).unwrap().output;
        assert_eq!(reset.address, None);
        assert_eq!(reset.mailed_to.as_deref(), Some("member-ken@example.org"));

        let sent = mailer.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to, "member-ken@example.org");
        let done = reset_password(&core, &Principal::Anonymous, new_password(&link_in(&sent[0]))).unwrap();
        assert_eq!(done.output.id, *id);
    }

    #[test]
    fn a_link_whose_mail_could_not_be_sent_does_not_work() {
        let (core, mailer) = fresh_with_mail(FakeMailer::broken());
        let admin = member(&core, "Ada", true);

        let failed = invite_member(&core, &admin, Invite { email: "grace@example.org".into() }).unwrap_err();
        assert_eq!(failed.kind(), "internal");
        // Were the mail delivered after all, its link would not work.
        let attempted = mailer.sent();
        let refused = accept_invitation(&core, &Principal::Anonymous, joining(&link_in(&attempted[0]), "Grace")).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
    }

    fn asks_for_reset(core: &Core, email: &str) -> Result<Reply<()>, OpError> {
        request_password_reset(core, &Principal::Anonymous, ResetRequest { email: email.into() })
    }

    #[test]
    fn with_mail_a_member_asks_for_a_reset_link_and_receives_it_by_mail() {
        let (core, mailer) = fresh_with_mail(FakeMailer::default());
        member(&core, "Ken", false);

        asks_for_reset(&core, " Member-Ken@Example.org ").unwrap();

        let sent = mailer.wait_for(1);
        assert_eq!(sent[0].to, "member-ken@example.org");
        let done = reset_password(&core, &Principal::Anonymous, new_password(&link_in(&sent[0]))).unwrap();
        assert_eq!(done.output.id, "member-ken");
    }

    #[test]
    fn asking_for_a_reset_for_an_unknown_or_deactivated_address_answers_alike_and_mails_nobody() {
        let (core, mailer) = fresh_with_mail(FakeMailer::default());
        let admin = member(&core, "Ada", true);
        member(&core, "Ken", false);
        member(&core, "Barbara", false);
        deactivate_member(&core, &admin, MemberId { id: "member-barbara".into() }).unwrap();

        let unknown = asks_for_reset(&core, "nobody@example.org").unwrap();
        let deactivated = asks_for_reset(&core, "member-barbara@example.org").unwrap();
        let known = asks_for_reset(&core, "member-ken@example.org").unwrap();
        assert_eq!((unknown.output, &unknown.session), (known.output, &known.session));
        assert_eq!((deactivated.output, &deactivated.session), ((), &SessionChange::Keep));

        // Mail goes out in the background; once Ken's has, no other is coming.
        let sent = mailer.wait_for(1);
        let recipients: Vec<_> = sent.iter().map(|mail| mail.to.as_str()).collect();
        assert_eq!(recipients, ["member-ken@example.org"]);
    }

    #[test]
    fn asking_again_soon_after_sends_no_second_mail() {
        let (core, mailer) = fresh_with_mail(FakeMailer::default());
        member(&core, "Ken", false);

        asks_for_reset(&core, "member-ken@example.org").unwrap();
        mailer.wait_for(1);
        asks_for_reset(&core, "member-ken@example.org").unwrap();
        core.advance(RESET_REQUEST_INTERVAL + 1);
        asks_for_reset(&core, "member-ken@example.org").unwrap();

        assert_eq!(mailer.wait_for(2).len(), 2);
    }

    #[test]
    fn without_mail_a_reset_cannot_be_asked_for() {
        let core = fresh();
        member(&core, "Ken", false);

        let refused = asks_for_reset(&core, "member-ken@example.org").unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
    }
}
