//! Permissions: answers what Role a principal holds on a node. Operations ask
//! here; nothing else decides access.
//!
//! A Grant gives a Role on a node to a holder. A node takes its Grants from
//! the nearest node at or above it that carries any (Inheritance). In the MVP
//! only an Area's root carries Grants, but the lookup already walks upward, so
//! giving a Subtree its own Grants later needs no change here.

use super::Principal;
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The level of access on an Area or Subtree. Each includes the one before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Read,
    Write,
    Admin,
}

impl Role {
    fn stored(self) -> &'static str {
        match self {
            Role::Read => "read",
            Role::Write => "write",
            Role::Admin => "admin",
        }
    }

    fn from_stored(stored: &str) -> Option<Role> {
        match stored {
            "read" => Some(Role::Read),
            "write" => Some(Role::Write),
            "admin" => Some(Role::Admin),
            _ => None,
        }
    }

    fn from_rank(rank: i64) -> Option<Role> {
        match rank {
            1 => Some(Role::Read),
            2 => Some(Role::Write),
            3 => Some(Role::Admin),
            _ => None,
        }
    }
}

/// Who a Grant is given to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Holder {
    Member(String),
    /// A standalone Agent.
    Agent(String),
    /// Every Member; never a standalone Agent.
    AllMembers,
}

impl Holder {
    fn stored(&self) -> String {
        match self {
            Holder::Member(id) => format!("member:{id}"),
            Holder::Agent(id) => format!("agent:{id}"),
            Holder::AllMembers => "all_members".into(),
        }
    }

    fn from_stored(stored: &str) -> Option<Holder> {
        match stored.split_once(':') {
            Some(("member", id)) => Some(Holder::Member(id.to_string())),
            Some(("agent", id)) => Some(Holder::Agent(id.to_string())),
            None if stored == "all_members" => Some(Holder::AllMembers),
            _ => None,
        }
    }
}

/// Gives a holder a Role on a node, replacing any Role they held there.
pub fn grant(db: &Connection, node_id: &str, holder: &Holder, role: Role) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO grants (node_id, holder, role) VALUES (?1, ?2, ?3)
         ON CONFLICT (node_id, holder) DO UPDATE SET role = excluded.role",
        rusqlite::params![node_id, holder.stored(), role.stored()],
    )?;
    Ok(())
}

/// Takes away the Grant a holder has on a node. Says whether there was one.
pub fn revoke(db: &Connection, node_id: &str, holder: &Holder) -> rusqlite::Result<bool> {
    let removed = db.execute(
        "DELETE FROM grants WHERE node_id = ?1 AND holder = ?2",
        rusqlite::params![node_id, holder.stored()],
    )?;
    Ok(removed > 0)
}

/// The Grants given on a node itself, not the ones it inherits.
pub fn grants_on(db: &Connection, node_id: &str) -> anyhow::Result<Vec<(Holder, Role)>> {
    let mut statement = db.prepare("SELECT holder, role FROM grants WHERE node_id = ?1")?;
    let rows = statement.query_map([node_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    let mut grants = Vec::new();
    for row in rows {
        let (holder, role) = row?;
        let parsed = Holder::from_stored(&holder).zip(Role::from_stored(&role));
        grants.push(parsed.ok_or_else(|| anyhow::anyhow!("node {node_id} has the unreadable Grant {holder} {role}"))?);
    }
    Ok(grants)
}

/// The Role a principal holds on a node, or `None` for no access. A node that
/// does not exist gives no access to anyone. A Token only ever lowers what its
/// owner holds: to nothing outside its Areas, to Read if it is read-only.
pub fn role_on(db: &Connection, principal: &Principal, node_id: &str) -> rusqlite::Result<Option<Role>> {
    let role = owners_role_on(db, principal, node_id)?;
    let Some(token) = principal.token() else { return Ok(role) };
    if let Some(areas) = &token.areas {
        let area_id: Option<String> = db
            .query_row("SELECT area_id FROM nodes WHERE id = ?1", [node_id], |row| row.get(0))
            .optional()?;
        if !area_id.is_some_and(|area_id| areas.contains(&area_id)) {
            return Ok(None);
        }
    }
    Ok(if token.read_only { role.map(|role| role.min(Role::Read)) } else { role })
}

/// The Role the Member or Agent behind a principal holds on a node, whatever
/// Token they act through.
fn owners_role_on(db: &Connection, principal: &Principal, node_id: &str) -> rusqlite::Result<Option<Role>> {
    let holders = holders_of(principal);
    if holders.is_empty() {
        return Ok(None);
    }
    if matches!(principal, Principal::Member { workspace_admin: true, .. }) {
        let exists: bool = db.query_row(
            "SELECT EXISTS (SELECT 1 FROM nodes WHERE id = ?1)",
            [node_id],
            |row| row.get(0),
        )?;
        return Ok(exists.then_some(Role::Admin));
    }
    // The highest Role among the Grants that apply wins.
    let rank: Option<i64> = db
        .query_row(
            "WITH RECURSIVE upward (id, parent_id, depth) AS (
                 SELECT id, parent_id, 0 FROM nodes WHERE id = ?1
                 UNION ALL
                 SELECT nodes.id, nodes.parent_id, upward.depth + 1
                 FROM nodes JOIN upward ON nodes.id = upward.parent_id
             ),
             governing AS (
                 SELECT id FROM upward
                 WHERE EXISTS (SELECT 1 FROM grants WHERE grants.node_id = upward.id)
                 ORDER BY depth LIMIT 1
             )
             SELECT MAX(CASE role WHEN 'read' THEN 1 WHEN 'write' THEN 2 WHEN 'admin' THEN 3 END)
             FROM grants
             WHERE node_id = (SELECT id FROM governing) AND holder IN (SELECT value FROM json_each(?2))",
            rusqlite::params![node_id, serde_json::to_string(&holders).map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(rank.and_then(Role::from_rank))
}

/// The node whose Grants apply to this one: the nearest at or above it that
/// carries any, or the root of its Area when none does. `None` when the node
/// does not exist.
///
/// Search stores this with each Page. In the MVP only roots carry Grants, so
/// it never changes; once Subtrees carry their own, giving a node its first
/// Grant or taking its last must mark the Pages beneath it as changed.
pub fn governing_node(db: &Connection, node_id: &str) -> rusqlite::Result<Option<String>> {
    db.query_row(
        "WITH RECURSIVE upward (id, parent_id, depth) AS (
             SELECT id, parent_id, 0 FROM nodes WHERE id = ?1
             UNION ALL
             SELECT nodes.id, nodes.parent_id, upward.depth + 1
             FROM nodes JOIN upward ON nodes.id = upward.parent_id
         )
         SELECT id FROM upward
         WHERE parent_id IS NULL
            OR EXISTS (SELECT 1 FROM grants WHERE grants.node_id = upward.id)
         ORDER BY depth LIMIT 1",
        [node_id],
        |row| row.get(0),
    )
    .optional()
}

/// The nodes whose Grants let the principal read: everything a node in this
/// list governs (see `governing_node`) is readable to them, and nothing else
/// is. `None` for a Workspace Admin, who reads everything.
pub fn readable_through(db: &Connection, principal: &Principal) -> rusqlite::Result<Option<Vec<String>>> {
    if matches!(principal, Principal::Member { workspace_admin: true, .. }) {
        return Ok(None);
    }
    let holders = holders_of(principal);
    if holders.is_empty() {
        return Ok(Some(Vec::new()));
    }
    // Every Role includes Read, so any Grant that applies to them will do.
    let mut statement =
        db.prepare("SELECT DISTINCT node_id FROM grants WHERE holder IN (SELECT value FROM json_each(?1))")?;
    let holders = serde_json::to_string(&holders).map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?;
    let nodes = statement
        .query_map([holders], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(Some(nodes))
}

/// The stored holders whose Grants apply to a principal: a Member's own and
/// those to all Members, or a standalone Agent's own alone.
fn holders_of(principal: &Principal) -> Vec<String> {
    match principal {
        Principal::Anonymous => Vec::new(),
        Principal::Member { id, .. } => vec![Holder::Member(id.clone()).stored(), Holder::AllMembers.stored()],
        Principal::Agent { id, .. } => vec![Holder::Agent(id.clone()).stored()],
    }
}
