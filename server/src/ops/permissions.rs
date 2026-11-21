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
    AllMembers,
}

impl Holder {
    fn stored(&self) -> String {
        match self {
            Holder::Member(id) => format!("member:{id}"),
            Holder::AllMembers => "all_members".into(),
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

/// The Role a principal holds on a node, or `None` for no access. A node that
/// does not exist gives no access to anyone.
pub fn role_on(db: &Connection, principal: &Principal, node_id: &str) -> rusqlite::Result<Option<Role>> {
    let (member_id, workspace_admin) = match principal {
        Principal::Anonymous => return Ok(None),
        Principal::Member {
            id,
            workspace_admin,
            ..
        } => (id, *workspace_admin),
    };
    if workspace_admin {
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
             WHERE node_id = (SELECT id FROM governing) AND holder IN (?2, ?3)",
            rusqlite::params![
                node_id,
                Holder::Member(member_id.clone()).stored(),
                Holder::AllMembers.stored()
            ],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(rank.and_then(Role::from_rank))
}
