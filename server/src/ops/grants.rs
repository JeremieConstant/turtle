//! Grants on an Area: who holds which Role there, as its Admins manage it.
//!
//! Grants are given on the root of an Area, so the whole Area inherits them.

use super::areas::{self, Area};
use super::permissions::{self, Holder, Role};
use super::tree::root_of;
use super::{Catalogue, Core, OpError, OpResult, Principal};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "list_grants",
            "Lists the Grants of an Area, and the active Members and standalone Agents one can be given to. Needs the Admin Role on the Area.",
            list_grants,
        )
        .changes = false;
    catalogue.add(
        "set_grant",
        "Gives a Member, a standalone Agent, or all Members at once, a Role on an Area, replacing the Role they held there. A Grant to all Members does not include standalone Agents. Someone holding several Grants gets the highest Role. The last Admin of an Area who is a person keeps that Role. Needs the Admin Role on the Area.",
        set_grant,
    );
    catalogue.add(
        "remove_grant",
        "Takes away the Grant of a Member, a standalone Agent, or all Members, on an Area. The last Admin of an Area who is a person keeps that Role. Needs the Admin Role on the Area.",
        remove_grant,
    );
}

/// Who a Grant is given to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GrantHolder {
    Member { member_id: String },
    /// A standalone Agent.
    Agent { agent_id: String },
    /// Every Member of the Workspace, including those who join later, but
    /// no standalone Agent.
    AllMembers,
}

/// One Grant on an Area.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Grant {
    pub holder: GrantHolder,
    /// The Member's or Agent's name, or "All Members".
    pub name: String,
    pub role: Role,
}

/// A Member who can be given a Grant. Only the name: email addresses are
/// for Workspace Admins, and any Member can be the Admin of an Area.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct GrantableMember {
    pub id: String,
    pub name: String,
}

/// A standalone Agent that can be given a Grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct GrantableAgent {
    pub id: String,
    pub name: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct GrantList {
    pub area: Area,
    /// All Members first, then each Member by name.
    pub grants: Vec<Grant>,
    /// Every active Member by name, to choose whom to give a Role.
    pub members: Vec<GrantableMember>,
    /// Every active standalone Agent by name, likewise.
    pub agents: Vec<GrantableAgent>,
}

/// What a change of Grants leaves the caller with.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RoleAfterwards {
    /// The Role the caller now holds on the Area; absent once they have none.
    pub role: Option<Role>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GrantsOfArea {
    pub area_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetGrant {
    pub area_id: String,
    pub holder: GrantHolder,
    pub role: Role,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RemoveGrant {
    pub area_id: String,
    pub holder: GrantHolder,
}

pub fn list_grants(core: &Core, principal: &Principal, input: GrantsOfArea) -> OpResult<GrantList> {
    let db = core.db();
    let area = administered_area(&db, principal, &input.area_id)?;
    Ok(grant_list(&db, area)?.into())
}

pub fn set_grant(core: &Core, principal: &Principal, input: SetGrant) -> OpResult<RoleAfterwards> {
    let mut db = core.db();
    let transaction = db.transaction()?;
    let area = administered_area(&transaction, principal, &input.area_id)?;
    let holder = holder_of(&transaction, &input.holder)?;
    let root_id = root_of(&transaction, &area.id)?;
    permissions::grant(&transaction, &root_id, &holder, input.role)?;
    keep_one_admin(&transaction, &root_id)?;
    transaction.commit()?;
    core.access_changed();
    role_afterwards(&db, principal, &root_id)
}

pub fn remove_grant(core: &Core, principal: &Principal, input: RemoveGrant) -> OpResult<RoleAfterwards> {
    let mut db = core.db();
    let transaction = db.transaction()?;
    let area = administered_area(&transaction, principal, &input.area_id)?;
    // Unlike giving one, removing a deactivated Member's or Agent's Grant is fine.
    let holder = Holder::from(&input.holder);
    let root_id = root_of(&transaction, &area.id)?;
    if !permissions::revoke(&transaction, &root_id, &holder)? {
        return Err(OpError::NotFound("There is no such Grant on this Area.".into()));
    }
    keep_one_admin(&transaction, &root_id)?;
    transaction.commit()?;
    core.access_changed();
    role_afterwards(&db, principal, &root_id)
}

/// Refuses a change that would leave an Area without an Admin who can act:
/// all Members, or a Member who is not deactivated. Workspace Admins do not
/// count, since they reach every Area without a Grant, and neither do
/// standalone Agents: a person has to stay able to manage the Area.
fn keep_one_admin(db: &Connection, root_id: &str) -> Result<(), OpError> {
    let any_admin: bool = db.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM grants
             WHERE node_id = ?1 AND role = 'admin'
               AND (holder = 'all_members'
                    OR holder IN (SELECT 'member:' || id FROM members WHERE deactivated_at IS NULL))
         )",
        [root_id],
        |row| row.get(0),
    )?;
    if !any_admin {
        return Err(OpError::Conflict(
            "An Area needs at least one Admin. Make someone else Admin first.".into(),
        ));
    }
    Ok(())
}

fn role_afterwards(db: &Connection, principal: &Principal, root_id: &str) -> OpResult<RoleAfterwards> {
    let role = permissions::role_on(db, principal, root_id)?;
    Ok(RoleAfterwards { role }.into())
}

/// An Area the principal is Admin of, for managing its Grants.
fn administered_area(db: &Connection, principal: &Principal, area_id: &str) -> Result<Area, OpError> {
    areas::signed_in(principal)?;
    areas::administered_area(db, principal, area_id, "Only an Admin of this Area can manage who has access to it.")
}

impl From<&GrantHolder> for Holder {
    fn from(holder: &GrantHolder) -> Holder {
        match holder {
            GrantHolder::AllMembers => Holder::AllMembers,
            GrantHolder::Member { member_id } => Holder::Member(member_id.clone()),
            GrantHolder::Agent { agent_id } => Holder::Agent(agent_id.clone()),
        }
    }
}

/// The holder to store, once a Member or Agent given by ID is known to be active.
fn holder_of(db: &Connection, holder: &GrantHolder) -> Result<Holder, OpError> {
    let (table, id, missing) = match holder {
        GrantHolder::AllMembers => return Ok(holder.into()),
        GrantHolder::Member { member_id } => ("members", member_id, "There is no such Member."),
        GrantHolder::Agent { agent_id } => ("agents", agent_id, "There is no such Agent."),
    };
    let active: bool = db.query_row(
        &format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE id = ?1 AND deactivated_at IS NULL)"),
        [id],
        |row| row.get(0),
    )?;
    if !active {
        return Err(OpError::NotFound(missing.into()));
    }
    Ok(holder.into())
}

fn grant_list(db: &Connection, area: Area) -> Result<GrantList, OpError> {
    let root_id = root_of(db, &area.id)?;
    let mut grants = Vec::new();
    for (holder, role) in permissions::grants_on(db, &root_id)? {
        grants.push(match holder {
            Holder::AllMembers => Grant { holder: GrantHolder::AllMembers, name: "All Members".into(), role },
            Holder::Member(member_id) => {
                let name = db.query_row("SELECT name FROM members WHERE id = ?1", [&member_id], |row| row.get(0))?;
                Grant { holder: GrantHolder::Member { member_id }, name, role }
            }
            Holder::Agent(agent_id) => {
                let name = db.query_row("SELECT name FROM agents WHERE id = ?1", [&agent_id], |row| row.get(0))?;
                Grant { holder: GrantHolder::Agent { agent_id }, name, role }
            }
        });
    }
    grants.sort_by(|a, b| {
        let all_members_first = |grant: &Grant| grant.holder != GrantHolder::AllMembers;
        (all_members_first(a), a.name.to_lowercase()).cmp(&(all_members_first(b), b.name.to_lowercase()))
    });
    let mut statement = db.prepare(
        "SELECT id, name FROM members WHERE deactivated_at IS NULL ORDER BY name COLLATE NOCASE, id",
    )?;
    let members = statement
        .query_map([], |row| Ok(GrantableMember { id: row.get(0)?, name: row.get(1)? }))?
        .collect::<Result<_, _>>()?;
    let mut statement = db.prepare(
        "SELECT id, name FROM agents WHERE deactivated_at IS NULL ORDER BY name COLLATE NOCASE, id",
    )?;
    let agents = statement
        .query_map([], |row| Ok(GrantableAgent { id: row.get(0)?, name: row.get(1)? }))?
        .collect::<Result<_, _>>()?;
    Ok(GrantList { area, grants, members, agents })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::areas::{create_area, NewArea};
    use crate::ops::testing::{fresh, member};

    fn member_id(principal: &Principal) -> String {
        let Principal::Member { id, .. } = principal else { panic!("not a Member") };
        id.clone()
    }

    fn holder(principal: &Principal) -> GrantHolder {
        GrantHolder::Member { member_id: member_id(principal) }
    }

    fn area(core: &Core, admin: &Principal) -> String {
        create_area(core, admin, NewArea { name: "Engineering".into() }).unwrap().output.id
    }

    #[test]
    fn an_area_admin_gives_a_member_a_role_and_sees_it_in_the_list() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area_id = area(&core, &grace);

        let holder = holder(&ken);
        set_grant(&core, &grace, SetGrant { area_id: area_id.clone(), holder, role: Role::Write }).unwrap();

        let list = list_grants(&core, &grace, GrantsOfArea { area_id }).unwrap().output;
        let shown: Vec<_> = list.grants.iter().map(|grant| (grant.name.as_str(), grant.role)).collect();
        assert_eq!(shown, [("Grace", Role::Admin), ("Ken", Role::Write)]);
    }

    fn to(area_id: &str, holder: GrantHolder, role: Role) -> SetGrant {
        SetGrant { area_id: area_id.into(), holder, role }
    }

    fn shown(core: &Core, admin: &Principal, area_id: &str) -> Vec<(String, Role)> {
        let list = list_grants(core, admin, GrantsOfArea { area_id: area_id.into() }).unwrap().output;
        list.grants.into_iter().map(|grant| (grant.name, grant.role)).collect()
    }

    #[test]
    fn grants_for_a_member_and_for_all_members_are_changed_and_removed() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area_id = area(&core, &grace);
        let ken_holder = holder(&ken);
        set_grant(&core, &grace, to(&area_id, ken_holder.clone(), Role::Read)).unwrap();
        set_grant(&core, &grace, to(&area_id, GrantHolder::AllMembers, Role::Read)).unwrap();

        set_grant(&core, &grace, to(&area_id, ken_holder.clone(), Role::Admin)).unwrap();
        assert_eq!(
            shown(&core, &grace, &area_id),
            [("All Members".into(), Role::Read), ("Grace".into(), Role::Admin), ("Ken".into(), Role::Admin)]
        );

        let removal = RemoveGrant { area_id: area_id.clone(), holder: GrantHolder::AllMembers };
        remove_grant(&core, &grace, removal).unwrap();
        let removal = RemoveGrant { area_id: area_id.clone(), holder: ken_holder };
        let afterwards = remove_grant(&core, &grace, removal).unwrap().output;
        assert_eq!(afterwards.role, Some(Role::Admin));
        assert_eq!(shown(&core, &grace, &area_id), [("Grace".into(), Role::Admin)]);
    }

    #[test]
    fn the_last_admin_of_an_area_can_be_neither_removed_nor_downgraded() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area_id = area(&core, &grace);
        let grace_holder = holder(&grace);

        let downgraded = set_grant(&core, &grace, to(&area_id, grace_holder.clone(), Role::Write)).unwrap_err();
        let removal = RemoveGrant { area_id: area_id.clone(), holder: grace_holder };
        let removed = remove_grant(&core, &grace, removal).unwrap_err();

        assert_eq!(downgraded.kind(), "conflict");
        assert_eq!(removed.kind(), "conflict");
        assert_eq!(shown(&core, &grace, &area_id), [("Grace".into(), Role::Admin)]);
    }

    #[test]
    fn an_admin_steps_down_once_someone_else_is_admin_and_then_holds_the_lower_role() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area_id = area(&core, &grace);
        let ken_holder = holder(&ken);
        set_grant(&core, &grace, to(&area_id, ken_holder, Role::Admin)).unwrap();

        let grace_holder = holder(&grace);
        let afterwards = set_grant(&core, &grace, to(&area_id, grace_holder, Role::Write)).unwrap().output;

        assert_eq!(afterwards.role, Some(Role::Write));
        assert_eq!(list_grants(&core, &grace, GrantsOfArea { area_id: area_id.clone() }).unwrap_err().kind(), "not_permitted");
        assert_eq!(shown(&core, &ken, &area_id), [("Grace".into(), Role::Write), ("Ken".into(), Role::Admin)]);
    }

    #[test]
    fn an_admin_grant_for_all_members_counts_and_one_of_a_deactivated_member_does_not() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let gone = member(&core, "Gone", false);
        let area_id = area(&core, &grace);
        let gone_holder = holder(&gone);
        set_grant(&core, &grace, to(&area_id, gone_holder, Role::Admin)).unwrap();
        core.db().execute("UPDATE members SET deactivated_at = 1 WHERE id = ?1", [member_id(&gone)]).unwrap();
        let grace_holder = holder(&grace);

        let refused = set_grant(&core, &grace, to(&area_id, grace_holder.clone(), Role::Read)).unwrap_err();
        assert_eq!(refused.kind(), "conflict");

        set_grant(&core, &grace, to(&area_id, GrantHolder::AllMembers, Role::Admin)).unwrap();
        let removal = RemoveGrant { area_id: area_id.clone(), holder: grace_holder };
        remove_grant(&core, &grace, removal).unwrap();
    }

    fn agent(core: &Core, name: &str) -> String {
        let ada = member(core, "Ada", true);
        let input = crate::ops::agents::NewAgent { name: name.into() };
        crate::ops::agents::create_agent(core, &ada, input).unwrap().output.id
    }

    #[test]
    fn an_area_admin_gives_a_standalone_agent_a_role_like_a_member() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let reviewer = agent(&core, "Reviewer");
        let area_id = area(&core, &grace);

        let choices = list_grants(&core, &grace, GrantsOfArea { area_id: area_id.clone() }).unwrap().output;
        assert_eq!(choices.agents, [GrantableAgent { id: reviewer.clone(), name: "Reviewer".into() }]);
        let holder = GrantHolder::Agent { agent_id: reviewer.clone() };
        set_grant(&core, &grace, to(&area_id, holder.clone(), Role::Write)).unwrap();

        let list = list_grants(&core, &grace, GrantsOfArea { area_id: area_id.clone() }).unwrap().output;
        let reviewer_grant = list.grants.iter().find(|grant| grant.holder == holder).unwrap();
        assert_eq!((reviewer_grant.name.as_str(), reviewer_grant.role), ("Reviewer", Role::Write));
        remove_grant(&core, &grace, RemoveGrant { area_id: area_id.clone(), holder }).unwrap();
        assert_eq!(shown(&core, &grace, &area_id), [("Grace".into(), Role::Admin)]);
    }

    #[test]
    fn a_standalone_agent_does_not_count_as_the_admin_a_person_needs_and_a_deactivated_one_gets_no_grant() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let reviewer = agent(&core, "Reviewer");
        let area_id = area(&core, &grace);
        let reviewer_holder = GrantHolder::Agent { agent_id: reviewer.clone() };
        set_grant(&core, &grace, to(&area_id, reviewer_holder, Role::Admin)).unwrap();

        let stepping_down = set_grant(&core, &grace, to(&area_id, holder(&grace), Role::Write)).unwrap_err();
        assert_eq!(stepping_down.kind(), "conflict");

        let barbara = member(&core, "Barbara", true);
        crate::ops::agents::deactivate_agent(&core, &barbara, crate::ops::agents::AgentId { id: reviewer.clone() }).unwrap();
        let other = area(&core, &grace);
        let refused = set_grant(&core, &grace, to(&other, GrantHolder::Agent { agent_id: reviewer }, Role::Read)).unwrap_err();
        assert_eq!(refused.kind(), "not_found");
        let list = list_grants(&core, &grace, GrantsOfArea { area_id: other }).unwrap().output;
        assert!(list.agents.is_empty());
    }

    #[test]
    fn changing_grants_tells_open_editors_to_check_their_access_again() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let area_id = area(&core, &grace);
        let ken_holder = holder(&ken);
        let mut watching = core.watch_access();

        set_grant(&core, &grace, to(&area_id, ken_holder.clone(), Role::Write)).unwrap();
        assert!(watching.has_changed().unwrap());
        watching.mark_unchanged();

        list_grants(&core, &grace, GrantsOfArea { area_id: area_id.clone() }).unwrap();
        assert!(!watching.has_changed().unwrap());

        remove_grant(&core, &grace, RemoveGrant { area_id, holder: ken_holder }).unwrap();
        assert!(watching.has_changed().unwrap());
    }
}
