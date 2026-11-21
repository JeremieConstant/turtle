//! Standalone Agents: non-human participants with a name, Tokens and Roles
//! of their own, managed by Workspace Admins.

use super::accounts::{random_hex, Nothing};
use super::{Catalogue, Core, OpError, OpResult, Principal};
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue.add(
        "create_agent",
        "Creates a standalone Agent: a non-human participant with its own name and Tokens, which Area Admins give Roles like a Member. A Grant to all Members does not include it. Only for a Workspace Admin.",
        create_agent,
    );
    catalogue
        .add(
            "list_agents",
            "Lists every standalone Agent by name, deactivated ones included. Only for a Workspace Admin.",
            list_agents,
        )
        .changes = false;
    catalogue.add("rename_agent", "Renames a standalone Agent. Only for a Workspace Admin.", rename_agent);
    catalogue.add(
        "deactivate_agent",
        "Deactivates a standalone Agent: its Tokens stop working at once. Only for a Workspace Admin.",
        deactivate_agent,
    );
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewAgent {
    /// What the Agent is called wherever it appears, such as among a Version's authors.
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AgentId {
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RenameAgent {
    pub id: String,
    pub name: String,
}

/// A standalone Agent as a Workspace Admin sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AgentEntry {
    pub id: String,
    pub name: String,
    /// False once the Agent has been deactivated.
    pub active: bool,
    /// Seconds since the Unix epoch.
    pub created_at: i64,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AgentList {
    pub agents: Vec<AgentEntry>,
}

/// Long enough for any sensible name, short enough to fit a list.
const MAX_NAME_LENGTH: usize = 200;

const ENTRY_COLUMNS: &str = "id, name, deactivated_at IS NULL, created_at";

pub fn create_agent(core: &Core, principal: &Principal, input: NewAgent) -> OpResult<AgentEntry> {
    require_workspace_admin(principal)?;
    let name = valid_name(&input.name)?;
    let agent = AgentEntry { id: random_hex(16)?, name, active: true, created_at: core.now() };
    core.db().execute(
        "INSERT INTO agents (id, name, created_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![agent.id, agent.name, agent.created_at],
    )?;
    Ok(agent.into())
}

pub fn list_agents(core: &Core, principal: &Principal, _: Nothing) -> OpResult<AgentList> {
    require_workspace_admin(principal)?;
    let db = core.db();
    let mut statement =
        db.prepare(&format!("SELECT {ENTRY_COLUMNS} FROM agents ORDER BY name COLLATE NOCASE, id"))?;
    let agents = statement.query_map([], read_entry)?.collect::<Result<_, _>>()?;
    Ok(AgentList { agents }.into())
}

pub fn rename_agent(core: &Core, principal: &Principal, input: RenameAgent) -> OpResult<AgentEntry> {
    require_workspace_admin(principal)?;
    let db = core.db();
    let agent = agent_entry(&db, &input.id)?;
    let name = valid_name(&input.name)?;
    db.execute("UPDATE agents SET name = ?1 WHERE id = ?2", rusqlite::params![name, agent.id])?;
    Ok(AgentEntry { name, ..agent }.into())
}

pub fn deactivate_agent(core: &Core, principal: &Principal, input: AgentId) -> OpResult<AgentEntry> {
    require_workspace_admin(principal)?;
    let db = core.db();
    let agent = agent_entry(&db, &input.id)?;
    db.execute(
        "UPDATE agents SET deactivated_at = ?1 WHERE id = ?2 AND deactivated_at IS NULL",
        rusqlite::params![core.now(), agent.id],
    )?;
    // Open editors acting through its Tokens close at once.
    core.access_changed();
    Ok(AgentEntry { active: false, ..agent }.into())
}

/// A standalone Agent, active or not.
pub(super) fn agent_entry(db: &Connection, id: &str) -> Result<AgentEntry, OpError> {
    db.query_row(&format!("SELECT {ENTRY_COLUMNS} FROM agents WHERE id = ?1"), [id], read_entry)
        .optional()?
        .ok_or_else(|| OpError::NotFound("There is no such Agent.".into()))
}

fn read_entry(row: &rusqlite::Row) -> rusqlite::Result<AgentEntry> {
    Ok(AgentEntry { id: row.get(0)?, name: row.get(1)?, active: row.get(2)?, created_at: row.get(3)? })
}

/// Only a Workspace Admin manages standalone Agents.
pub(super) fn require_workspace_admin(principal: &Principal) -> Result<(), OpError> {
    super::require_workspace_admin(principal, "Only a Workspace Admin can manage Agents.")
}

fn valid_name(name: &str) -> Result<String, OpError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(OpError::InvalidInput("Enter a name for the Agent.".into()));
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return Err(OpError::InvalidInput(format!("Choose a name of at most {MAX_NAME_LENGTH} characters.")));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::accounts::Nothing;
    use crate::ops::testing::{fresh, member};

    fn named(name: &str) -> NewAgent {
        NewAgent { name: name.into() }
    }

    fn listed(core: &Core, admin: &Principal) -> Vec<(String, bool)> {
        let list = list_agents(core, admin, Nothing {}).unwrap().output;
        list.agents.into_iter().map(|agent| (agent.name, agent.active)).collect()
    }

    #[test]
    fn a_workspace_admin_creates_and_renames_agents_and_lists_them_by_name() {
        let core = fresh();
        let ada = member(&core, "Ada", true);

        let reviewer = create_agent(&core, &ada, named("Reviewer")).unwrap().output;
        create_agent(&core, &ada, named("  Archivist ")).unwrap();
        let renamed = rename_agent(&core, &ada, RenameAgent { id: reviewer.id.clone(), name: "Proofreader".into() })
            .unwrap()
            .output;

        assert_eq!(renamed.name, "Proofreader");
        assert_eq!(listed(&core, &ada), [("Archivist".into(), true), ("Proofreader".into(), true)]);
    }

    #[test]
    fn an_agent_needs_a_name() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        assert_eq!(create_agent(&core, &ada, named(" ")).unwrap_err().kind(), "invalid_input");
    }

    #[test]
    fn only_a_workspace_admin_manages_agents() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let grace = member(&core, "Grace", false);
        let agent = create_agent(&core, &ada, named("Reviewer")).unwrap().output;

        for caller in [&grace, &Principal::Anonymous] {
            assert_eq!(create_agent(&core, caller, named("Sneaky")).unwrap_err().kind(), "not_permitted");
            assert_eq!(list_agents(&core, caller, Nothing {}).unwrap_err().kind(), "not_permitted");
            let rename = RenameAgent { id: agent.id.clone(), name: "Mine".into() };
            assert_eq!(rename_agent(&core, caller, rename).unwrap_err().kind(), "not_permitted");
            let deactivate = AgentId { id: agent.id.clone() };
            assert_eq!(deactivate_agent(&core, caller, deactivate).unwrap_err().kind(), "not_permitted");
        }
        assert_eq!(listed(&core, &ada), [("Reviewer".into(), true)]);
    }

    #[test]
    fn a_deactivated_agent_stays_listed_as_inactive() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let agent = create_agent(&core, &ada, named("Reviewer")).unwrap().output;
        let watching = core.watch_access();

        let deactivated = deactivate_agent(&core, &ada, AgentId { id: agent.id.clone() }).unwrap().output;

        assert!(!deactivated.active);
        assert!(watching.has_changed().unwrap());
        assert_eq!(listed(&core, &ada), [("Reviewer".into(), false)]);
        let missing = AgentId { id: "no-such-agent".into() };
        assert_eq!(deactivate_agent(&core, &ada, missing).unwrap_err().kind(), "not_found");
    }
}
