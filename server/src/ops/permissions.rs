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
    // What is in the Trash is out of reach until it is restored, whatever
    // Role was held on it; the Trash's own operations ask about its Area.
    if in_trash(db, node_id)? {
        return Ok(None);
    }
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

/// Whether a node is in the Trash: it, or a node above it, was moved there,
/// or its Area was deleted.
pub fn in_trash(db: &Connection, node_id: &str) -> rusqlite::Result<bool> {
    db.query_row(
        "WITH RECURSIVE upward (id, parent_id, area_id) AS (
             SELECT id, parent_id, area_id FROM nodes WHERE id = ?1
             UNION ALL
             SELECT nodes.id, nodes.parent_id, nodes.area_id
             FROM nodes JOIN upward ON nodes.id = upward.parent_id
         )
         SELECT EXISTS (SELECT 1 FROM upward JOIN trash ON trash.node_id = upward.id)
             OR EXISTS (SELECT 1 FROM upward JOIN deleted_areas ON deleted_areas.area_id = upward.area_id)",
        [node_id],
        |row| row.get(0),
    )
}

/// The Role the Member or Agent behind a principal holds on a node, whatever
/// Token they act through.
fn owners_role_on(db: &Connection, principal: &Principal, node_id: &str) -> rusqlite::Result<Option<Role>> {
    let holders = holders_of(db, principal)?;
    if holders.is_empty() {
        return Ok(None);
    }
    // A Personal Area answers to its Member alone (ADR-0008): no Grant
    // reaches into it, and being Workspace Admin does not either.
    if let Some(owner) = personal_member_of(db, node_id)? {
        return Ok(matches!(principal, Principal::Member { id, .. } if *id == owner).then_some(Role::Admin));
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

/// The Member whose Personal Area the node is in, or `None` when it is in
/// another Area or does not exist.
pub fn personal_member_of(db: &Connection, node_id: &str) -> rusqlite::Result<Option<String>> {
    let owner = db
        .query_row(
            "SELECT areas.personal_member_id FROM nodes JOIN areas ON areas.id = nodes.area_id WHERE nodes.id = ?1",
            [node_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?;
    Ok(owner.flatten())
}

/// The node whose Grants apply to this one: the nearest at or above it that
/// carries any, or the root of its Area when none does. `None` when the node
/// does not exist.
///
/// It says nothing about the Trash, which `role_on` also refuses: a caller
/// deciding access with it leaves out what `in_trash` reports itself.
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
/// is. A Member reads through the root of their Personal Area, and a
/// Workspace Admin through every node that governs any other Area's Pages.
/// A Token limited to chosen Areas reads through the nodes in those alone.
/// Like `governing_node`, it leaves the Trash to the caller (see `in_trash`).
pub fn readable_through(db: &Connection, principal: &Principal) -> rusqlite::Result<Vec<String>> {
    let nodes = owner_reads_through(db, principal)?;
    let Some(areas) = principal.token().and_then(|token| token.areas.as_ref()) else { return Ok(nodes) };
    let mut statement = db.prepare("SELECT area_id FROM nodes WHERE id = ?1")?;
    let mut within = Vec::new();
    for node in nodes {
        let area_id: String = statement.query_row([&node], |row| row.get(0))?;
        if areas.contains(&area_id) {
            within.push(node);
        }
    }
    Ok(within)
}

/// What `readable_through` gives the Member or Agent behind a principal,
/// whatever Token they act through.
fn owner_reads_through(db: &Connection, principal: &Principal) -> rusqlite::Result<Vec<String>> {
    let (holders, member_id) = match principal {
        Principal::Anonymous => return Ok(Vec::new()),
        Principal::Member { id, workspace_admin: true, .. } => {
            let mut statement = db.prepare(
                "SELECT nodes.id FROM nodes JOIN areas ON areas.id = nodes.area_id
                 WHERE (nodes.parent_id IS NULL OR EXISTS (SELECT 1 FROM grants WHERE grants.node_id = nodes.id))
                   AND (areas.personal_member_id IS NULL OR areas.personal_member_id = ?1)",
            )?;
            return statement.query_map([id], |row| row.get(0))?.collect();
        }
        Principal::Member { id, .. } => (holders_of(db, principal)?, Some(id)),
        Principal::Agent { .. } => (holders_of(db, principal)?, None),
    };
    // Every Role includes Read, so any Grant that applies to them will do.
    let mut statement = db.prepare(
        "SELECT DISTINCT node_id FROM grants WHERE holder IN (SELECT value FROM json_each(?1))
         UNION
         SELECT nodes.id FROM nodes JOIN areas ON areas.id = nodes.area_id
         WHERE nodes.parent_id IS NULL AND areas.personal_member_id = ?2",
    )?;
    let holders = serde_json::to_string(&holders).map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?;
    let nodes = statement.query_map(rusqlite::params![holders, member_id], |row| row.get(0))?;
    nodes.collect()
}

/// Whether the principal is a Guest: a Member account of an outside person.
pub fn is_guest(db: &Connection, principal: &Principal) -> rusqlite::Result<bool> {
    let Principal::Member { id, .. } = principal else { return Ok(false) };
    let guest = db
        .query_row("SELECT guest FROM members WHERE id = ?1", [id], |row| row.get(0))
        .optional()?;
    Ok(guest.unwrap_or(false))
}

/// The stored holders whose Grants apply to a principal: a Member's own and
/// those to all Members, a Guest's own alone, or a standalone Agent's own alone.
fn holders_of(db: &Connection, principal: &Principal) -> rusqlite::Result<Vec<String>> {
    Ok(match principal {
        Principal::Anonymous => Vec::new(),
        Principal::Member { id, .. } if is_guest(db, principal)? => vec![Holder::Member(id.clone()).stored()],
        Principal::Member { id, .. } => vec![Holder::Member(id.clone()).stored(), Holder::AllMembers.stored()],
        Principal::Agent { id, .. } => vec![Holder::Agent(id.clone()).stored()],
    })
}
