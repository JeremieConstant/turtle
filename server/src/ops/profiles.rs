//! Profiles: what Turtle shows others about a Member or Guest, and who may
//! see whose. An Agent has no Profile.

use super::accounts::Nothing;
use super::permissions;
use super::{Catalogue, Core, OpError, OpResult, Principal};
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashSet;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "get_profile",
            "Returns the Profile of a Member or Guest by ID: name, email, whether they are a Guest or Workspace Admin, and whether they are deactivated. A Guest sees only the Profiles of people who can reach an Area or Subtree the Guest can reach; Agents have no Profile.",
            get_profile,
        )
        .changes = false;
    catalogue
        .add(
            "list_profiles",
            "Lists the Members and Guests whose Profiles the caller sees, by name, deactivated ones included. A Member sees everyone; a Guest only the people who can reach an Area or Subtree the Guest can reach. Agents have no Profile.",
            list_profiles,
        )
        .changes = false;
    catalogue
        .add(
            "get_mentioned_people",
            "Gets the current names of the Members and Guests that Mentions name, by ID, as Mentions show them: everyone is told the name, and the Profile only when the caller may see it. An ID that names no Member or Guest, such as an Agent's, is left out.",
            get_mentioned_people,
        )
        .changes = false;
}

/// What Turtle shows others about a Member or Guest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub email: String,
    /// An outside person invited to chosen Areas or Subtrees.
    pub guest: bool,
    pub workspace_admin: bool,
    /// False once they have been deactivated; their Profile stays.
    pub active: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProfileId {
    /// The Member's or Guest's ID.
    pub id: String,
}

pub fn get_profile(core: &Core, principal: &Principal, input: ProfileId) -> OpResult<Profile> {
    let db = core.db();
    let sight = Sight::of(&db, principal)?;
    match read_profile(&db, &input.id)? {
        Some(profile) if sight.sees(&db, principal, &profile)? => Ok(profile.into()),
        _ => Err(no_such_person()),
    }
}

/// The people whose Profiles the caller sees.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ProfileList {
    /// By name, deactivated people included.
    pub profiles: Vec<Profile>,
}

pub fn list_profiles(core: &Core, principal: &Principal, _: Nothing) -> OpResult<ProfileList> {
    let db = core.db();
    let sight = Sight::of(&db, principal)?;
    let mut statement = db.prepare(&format!(
        "SELECT {PROFILE_COLUMNS} FROM members ORDER BY name COLLATE NOCASE, id"
    ))?;
    let mut profiles = Vec::new();
    for profile in statement.query_map([], profile_row)? {
        let profile = profile?;
        if sight.sees(&db, principal, &profile)? {
            profiles.push(profile);
        }
    }
    Ok(ProfileList { profiles }.into())
}

/// The most people one call asks about, a generous number for one Page's Mentions.
const MAX_PEOPLE: usize = 500;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeopleToName {
    /// The IDs of the Members and Guests, as Mentions name them. Only the first 500 count.
    pub person_ids: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct MentionedPeople {
    /// The Members and Guests among those asked for, in the order asked, each once.
    pub people: Vec<MentionedPerson>,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct MentionedPerson {
    pub id: String,
    /// Their current name, which a Mention shows to everyone.
    pub name: String,
    /// Their Profile, when the caller may see it; otherwise a Mention shows
    /// the name alone.
    pub profile: Option<Profile>,
}

pub fn get_mentioned_people(core: &Core, principal: &Principal, input: PeopleToName) -> OpResult<MentionedPeople> {
    let db = core.db();
    let sight = Sight::of(&db, principal)?;
    let mut people: Vec<MentionedPerson> = Vec::new();
    for id in input.person_ids.into_iter().take(MAX_PEOPLE) {
        if people.iter().any(|person| person.id == id) {
            continue;
        }
        // A name is given for any ID, since Mentions show it to every reader
        // of the content; IDs are random, so only that content tells one.
        let Some(person) = read_profile(&db, &id)? else { continue };
        let profile = sight.sees(&db, principal, &person)?.then(|| person.clone());
        people.push(MentionedPerson { id, name: person.name, profile });
    }
    Ok(MentionedPeople { people }.into())
}

/// Whose Profiles a caller sees. This one rule decides both a single
/// Profile and the listing of people.
enum Sight {
    /// A Member sees everyone.
    Everyone,
    /// A Guest, or a standalone Agent, sees themselves and the people who
    /// can reach at least one Area or Subtree they reach. Each is named by
    /// the node whose Grants govern it.
    Sharing(HashSet<String>),
}

impl Sight {
    fn of(db: &Connection, principal: &Principal) -> Result<Sight, OpError> {
        match principal {
            Principal::Anonymous => Err(OpError::NotPermitted("Sign in first.".into())),
            Principal::Member { .. } if !permissions::is_guest(db, principal)? => Ok(Sight::Everyone),
            Principal::Member { .. } | Principal::Agent { .. } => Ok(Sight::Sharing(reach(db, principal)?)),
        }
    }

    fn sees(&self, db: &Connection, viewer: &Principal, person: &Profile) -> rusqlite::Result<bool> {
        let Sight::Sharing(reached) = self else { return Ok(true) };
        if matches!(viewer, Principal::Member { id, .. } if *id == person.id) {
            return Ok(true);
        }
        let as_person = Principal::Member {
            id: person.id.clone(),
            workspace_admin: person.workspace_admin,
            session: None,
            token: None,
        };
        Ok(!reach(db, &as_person)?.is_disjoint(reached))
    }
}

/// The nodes governing what a principal can read, leaving out what is in the
/// Trash, which nobody reaches.
fn reach(db: &Connection, principal: &Principal) -> rusqlite::Result<HashSet<String>> {
    let mut reached = HashSet::new();
    for node in permissions::readable_through(db, principal)? {
        if !permissions::in_trash(db, &node)? {
            reached.insert(node);
        }
    }
    Ok(reached)
}

const PROFILE_COLUMNS: &str = "id, name, email, guest, workspace_admin, deactivated_at IS NULL";

fn read_profile(db: &Connection, id: &str) -> rusqlite::Result<Option<Profile>> {
    db.query_row(&format!("SELECT {PROFILE_COLUMNS} FROM members WHERE id = ?1"), [id], profile_row)
        .optional()
}

fn profile_row(row: &rusqlite::Row) -> rusqlite::Result<Profile> {
    Ok(Profile {
        id: row.get(0)?,
        name: row.get(1)?,
        email: row.get(2)?,
        guest: row.get(3)?,
        workspace_admin: row.get(4)?,
        active: row.get(5)?,
    })
}

/// The current name of a Member or Guest, deactivated or not, whoever asks.
pub(super) fn name_of(db: &Connection, id: &str) -> rusqlite::Result<Option<String>> {
    Ok(read_profile(db, id)?.map(|profile| profile.name))
}

fn no_such_person() -> OpError {
    OpError::NotFound("There is no such Member or Guest.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::agents::{create_agent, NewAgent};
    use crate::ops::areas::{create_area, AreaId, NewArea};
    use crate::ops::members::{deactivate_member, MemberId};
    use crate::ops::trash::delete_area;
    use crate::ops::TokenLimits;
    use crate::ops::grants::{set_grant, GrantHolder, SetGrant};
    use crate::ops::permissions::Role;
    use crate::ops::testing::{fresh, guest, member};

    fn id_of(principal: &Principal) -> String {
        let Principal::Member { id, .. } = principal else { panic!("not a Member") };
        id.clone()
    }

    fn profile(core: &Core, viewer: &Principal, of: &Principal) -> Result<Profile, OpError> {
        get_profile(core, viewer, ProfileId { id: id_of(of) }).map(|reply| reply.output)
    }

    #[test]
    fn a_member_sees_another_members_profile() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let ken = member(&core, "Ken", false);
        assert_eq!(
            profile(&core, &ken, &ada).unwrap(),
            Profile {
                id: "member-ada".into(),
                name: "Ada".into(),
                email: "member-ada@example.org".into(),
                guest: false,
                workspace_admin: true,
                active: true,
            }
        );
    }

    fn area(core: &Core, admin: &Principal, name: &str) -> String {
        create_area(core, admin, NewArea { name: name.into() }).unwrap().output.id
    }

    fn give(core: &Core, admin: &Principal, area_id: &str, holder: GrantHolder) {
        let input = SetGrant { area_id: area_id.into(), holder, role: Role::Read };
        set_grant(core, admin, input).unwrap();
    }

    fn holder(principal: &Principal) -> GrantHolder {
        GrantHolder::Member { member_id: id_of(principal) }
    }

    #[test]
    fn a_guest_sees_only_the_people_who_reach_an_area_the_guest_reaches() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let ken = member(&core, "Ken", false);
        let gus = guest(&core, "Gus");
        let sales = area(&core, &ada, "Sales");
        area(&core, &ken, "Engineering");
        give(&core, &ada, &sales, holder(&gus));

        assert_eq!(profile(&core, &gus, &ada).unwrap().name, "Ada");
        assert_eq!(profile(&core, &gus, &gus).unwrap().name, "Gus");
        let hidden = profile(&core, &gus, &ken).unwrap_err();
        let missing = get_profile(&core, &gus, ProfileId { id: "nobody".into() }).unwrap_err();
        assert_eq!((hidden.kind(), hidden.message()), (missing.kind(), missing.message()));
        assert_eq!(hidden.kind(), "not_found");
    }

    #[test]
    fn a_grant_to_all_members_does_not_reach_a_guest() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let ken = member(&core, "Ken", false);
        let gus = guest(&core, "Gus");
        let sales = area(&core, &ada, "Sales");
        let engineering = area(&core, &ken, "Engineering");
        give(&core, &ada, &sales, holder(&gus));
        give(&core, &ken, &engineering, GrantHolder::AllMembers);

        assert_eq!(profile(&core, &gus, &ken).unwrap_err().kind(), "not_found");
    }

    #[test]
    fn a_guest_sees_the_members_who_reach_a_shared_area_through_a_grant_to_all_members() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let ken = member(&core, "Ken", false);
        let gus = guest(&core, "Gus");
        let sales = area(&core, &ada, "Sales");
        give(&core, &ada, &sales, holder(&gus));
        give(&core, &ada, &sales, GrantHolder::AllMembers);

        assert_eq!(profile(&core, &gus, &ken).unwrap().name, "Ken");
    }

    #[test]
    fn a_workspace_admin_sees_a_guests_profile_and_a_guest_sees_the_workspace_admins() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let wanda = member(&core, "Wanda", true);
        let gus = guest(&core, "Gus");
        let sales = area(&core, &ada, "Sales");
        give(&core, &ada, &sales, holder(&gus));

        let seen = profile(&core, &wanda, &gus).unwrap();
        assert!(seen.guest && !seen.workspace_admin);
        assert!(profile(&core, &gus, &wanda).unwrap().workspace_admin);
    }

    #[test]
    fn a_guest_without_any_area_sees_only_themselves() {
        let core = fresh();
        let wanda = member(&core, "Wanda", true);
        let gus = guest(&core, "Gus");
        assert_eq!(profile(&core, &gus, &wanda).unwrap_err().kind(), "not_found");
        assert_eq!(profile(&core, &gus, &gus).unwrap().name, "Gus");
    }

    #[test]
    fn an_area_in_the_trash_is_shared_by_nobody() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let gus = guest(&core, "Gus");
        let sales = area(&core, &ada, "Sales");
        give(&core, &ada, &sales, holder(&gus));
        delete_area(&core, &ada, AreaId { id: sales }).unwrap();

        assert_eq!(profile(&core, &gus, &ada).unwrap_err().kind(), "not_found");
    }

    #[test]
    fn a_deactivated_persons_profile_stays_and_says_so() {
        let core = fresh();
        let wanda = member(&core, "Wanda", true);
        let ken = member(&core, "Ken", false);
        let ada = member(&core, "Ada", false);
        deactivate_member(&core, &wanda, MemberId { id: id_of(&ken) }).unwrap();

        let seen = profile(&core, &ada, &ken).unwrap();
        assert_eq!((seen.name.as_str(), seen.active), ("Ken", false));
    }

    #[test]
    fn an_agent_has_no_profile() {
        let core = fresh();
        let wanda = member(&core, "Wanda", true);
        let agent = create_agent(&core, &wanda, NewAgent { name: "Reviewer".into() }).unwrap().output;
        let error = get_profile(&core, &wanda, ProfileId { id: agent.id }).unwrap_err();
        assert_eq!(error.kind(), "not_found");
    }

    #[test]
    fn a_standalone_agent_sees_the_people_who_reach_an_area_it_reaches() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let ken = member(&core, "Ken", false);
        let wanda = member(&core, "Wanda", true);
        let reviewer = create_agent(&core, &wanda, NewAgent { name: "Reviewer".into() }).unwrap().output;
        let sales = area(&core, &ada, "Sales");
        give(&core, &ada, &sales, GrantHolder::Agent { agent_id: reviewer.id.clone() });
        let token = TokenLimits { id: "token".into(), read_only: false, areas: None };
        let agent = Principal::Agent { id: reviewer.id, token };

        assert_eq!(profile(&core, &agent, &ada).unwrap().name, "Ada");
        assert_eq!(profile(&core, &agent, &ken).unwrap_err().kind(), "not_found");
    }

    #[test]
    fn nobody_signed_in_sees_no_profile() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        assert_eq!(profile(&core, &Principal::Anonymous, &ada).unwrap_err().kind(), "not_permitted");
    }

    fn names(core: &Core, viewer: &Principal) -> Vec<String> {
        let listed = list_profiles(core, viewer, Nothing {}).unwrap().output;
        listed.profiles.into_iter().map(|profile| profile.name).collect()
    }

    #[test]
    fn a_member_lists_every_member_and_guest_by_name_but_no_agent() {
        let core = fresh();
        let wanda = member(&core, "Wanda", true);
        let ada = member(&core, "ada", false);
        guest(&core, "Gus");
        create_agent(&core, &wanda, NewAgent { name: "Reviewer".into() }).unwrap();

        assert_eq!(names(&core, &ada), ["ada", "Gus", "Wanda"]);
    }

    #[test]
    fn a_guest_lists_only_the_people_whose_profiles_they_see() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        member(&core, "Ken", false);
        member(&core, "Wanda", true);
        let gus = guest(&core, "Gus");
        let sales = area(&core, &ada, "Sales");
        give(&core, &ada, &sales, holder(&gus));

        assert_eq!(names(&core, &gus), ["Ada", "Gus", "Wanda"]);
    }

    fn mentioned(core: &Core, reader: &Principal, ids: &[&str]) -> Vec<MentionedPerson> {
        let input = PeopleToName { person_ids: ids.iter().map(|id| id.to_string()).collect() };
        get_mentioned_people(core, reader, input).unwrap().output.people
    }

    #[test]
    fn a_mention_shows_everyone_the_name_and_the_profile_only_to_whoever_sees_it() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let ken = member(&core, "Ken", false);
        let wanda = member(&core, "Wanda", true);
        let gus = guest(&core, "Gus");
        let reviewer = create_agent(&core, &wanda, NewAgent { name: "Reviewer".into() }).unwrap().output;
        let sales = area(&core, &ada, "Sales");
        give(&core, &ada, &sales, holder(&gus));
        deactivate_member(&core, &wanda, MemberId { id: id_of(&ken) }).unwrap();

        let ids = ["member-ken", "member-ada", "member-ken", "nobody", reviewer.id.as_str()];
        let as_gus = mentioned(&core, &gus, &ids);
        assert_eq!(
            as_gus,
            [
                MentionedPerson { id: "member-ken".into(), name: "Ken".into(), profile: None },
                MentionedPerson {
                    id: "member-ada".into(),
                    name: "Ada".into(),
                    profile: Some(profile(&core, &gus, &ada).unwrap()),
                },
            ]
        );
        let as_ada = mentioned(&core, &ada, &ids);
        let ken_seen = as_ada[0].profile.as_ref().unwrap();
        assert_eq!((ken_seen.name.as_str(), ken_seen.active), ("Ken", false));
    }

    #[test]
    fn nobody_signed_in_is_told_no_names() {
        let core = fresh();
        member(&core, "Ada", false);
        let error = get_mentioned_people(&core, &Principal::Anonymous, PeopleToName { person_ids: vec!["member-ada".into()] })
            .unwrap_err();
        assert_eq!(error.kind(), "not_permitted");
    }
}
