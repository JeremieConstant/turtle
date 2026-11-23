//! Tokens: credentials a Member creates so an Agent can act as them, or a
//! Workspace Admin creates for a standalone Agent, used through the HTTP API
//! and the MCP endpoint, never with more access than their owner has.

use super::accounts::{digest, random_hex};
use super::{agents, areas};
use super::{Catalogue, Core, OpError, OpResult, Principal, TokenLimits};
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue
        .add(
            "create_token",
            "Creates a Token that lets an Agent act as the caller, or, for a Workspace Admin, one for a standalone Agent to act as itself; optionally only reading, only in chosen Areas, and until a date. Its value is shown only in this answer.",
            create_token,
        )
        .browser_only = true;
    {
        let operation = catalogue.add("list_tokens", "Lists the caller's Tokens, or a standalone Agent's for a Workspace Admin, with when each was last used.", list_tokens);
        operation.browser_only = true;
        operation.changes = false;
    }
    catalogue
        .add("revoke_token", "Revokes one of the caller's Tokens, or of a standalone Agent's for a Workspace Admin; it stops working at once.", revoke_token)
        .browser_only = true;
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewToken {
    /// What the Token is for, to tell it apart from the others.
    pub name: String,
    /// Whether nothing can be changed through it.
    #[serde(default)]
    pub read_only: bool,
    /// The only Areas it reaches; every Area the caller reaches if absent.
    #[serde(default)]
    pub area_ids: Option<Vec<String>>,
    /// When it stops working, in seconds since the Unix epoch; never if absent.
    #[serde(default)]
    pub expires_at: Option<i64>,
    /// The standalone Agent it is for; the caller's own if absent.
    #[serde(default)]
    pub agent_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct TokensOf {
    /// The standalone Agent whose Tokens to list; the caller's own if absent.
    #[serde(default)]
    pub agent_id: Option<String>,
}

/// An Area a Token is limited to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TokenArea {
    pub id: String,
    pub name: String,
}

/// A Token as its owner sees it. Its value is never shown again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TokenSummary {
    pub id: String,
    pub name: String,
    pub read_only: bool,
    /// The only Areas it reaches; every Area its owner reaches if absent.
    /// Areas the owner can no longer read are left out.
    pub areas: Option<Vec<TokenArea>>,
    /// When it stops working, in seconds since the Unix epoch.
    pub expires_at: Option<i64>,
    /// When it was last used, in seconds since the Unix epoch.
    pub last_used_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CreatedToken {
    pub token: TokenSummary,
    /// The value an Agent sends as `Authorization: Bearer <value>`. Shown
    /// only here; Turtle keeps only a hash of it.
    pub value: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TokenList {
    /// Newest first.
    pub tokens: Vec<TokenSummary>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TokenId {
    pub id: String,
    /// The standalone Agent the Token is for; the caller's own if absent.
    #[serde(default)]
    pub agent_id: Option<String>,
}

/// Whose Tokens a call manages.
enum Owner {
    Member(String),
    Agent(agents::AgentEntry),
}

impl Owner {
    /// The column of `tokens` that names this owner, and the value there.
    fn column(&self) -> (&'static str, &str) {
        match self {
            Owner::Member(id) => ("member_id", id),
            Owner::Agent(agent) => ("agent_id", &agent.id),
        }
    }
}

/// The caller themself, or the standalone Agent a Workspace Admin manages.
fn owner(db: &Connection, principal: &Principal, agent_id: Option<&str>) -> Result<Owner, OpError> {
    let Some(agent_id) = agent_id else {
        let member_id = areas::signed_in_member(
            principal,
            "A standalone Agent cannot manage Tokens. Ask a Workspace Admin to manage them in Turtle.",
        )?;
        return Ok(Owner::Member(member_id.to_string()));
    };
    agents::require_workspace_admin(principal)?;
    Ok(Owner::Agent(agents::agent_entry(db, agent_id)?))
}

/// Every Token value starts with this, so a leaked one is easy to recognise.
const VALUE_PREFIX: &str = "turtle_";

/// How exactly the last use of a Token is remembered, in seconds.
const LAST_USE_PRECISION: i64 = 60;

/// Long enough for any sensible name, short enough to fit a list.
const MAX_NAME_LENGTH: usize = 200;

pub fn create_token(core: &Core, principal: &Principal, input: NewToken) -> OpResult<CreatedToken> {
    let owner = owner(&core.db(), principal, input.agent_id.as_deref())?;
    if let Owner::Agent(agent) = &owner {
        if !agent.active {
            return Err(OpError::Conflict("A deactivated Agent cannot act, so it gets no new Token.".into()));
        }
    }
    let name = valid_name(&input.name)?;
    let now = core.now();
    if input.expires_at.is_some_and(|expires_at| expires_at <= now) {
        return Err(OpError::InvalidInput("Choose an expiry date in the future.".into()));
    }
    let id = random_hex(16)?;
    let value = format!("{VALUE_PREFIX}{}", random_hex(32)?);

    let mut db = core.db();
    let transaction = db.transaction()?;
    // Only Areas the caller can read, so a Token never reaches further. A
    // standalone Agent's is made by a Workspace Admin, who reads every Area.
    let mut chosen: Option<Vec<TokenArea>> = None;
    if let Some(area_ids) = &input.area_ids {
        if area_ids.is_empty() {
            return Err(OpError::InvalidInput(
                "Choose at least one Area, or let the Token reach every Area.".into(),
            ));
        }
        let mut found: Vec<TokenArea> = Vec::new();
        for area_id in area_ids {
            let area = areas::readable_area(&transaction, principal, area_id)?;
            if found.iter().all(|known| known.id != area.id) {
                found.push(TokenArea { id: area.id, name: area.name });
            }
        }
        chosen = Some(found);
    }
    let (owner_column, owner_id) = owner.column();
    transaction.execute(
        &format!(
            "INSERT INTO tokens (id, {owner_column}, name, secret_digest, read_only, all_areas, expires_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
        ),
        rusqlite::params![id, owner_id, name, digest(&value), input.read_only, chosen.is_none(), input.expires_at, now],
    )?;
    for area in chosen.iter().flatten() {
        transaction.execute(
            "INSERT INTO token_areas (token_id, area_id) VALUES (?1, ?2)",
            rusqlite::params![id, area.id],
        )?;
    }
    transaction.commit()?;
    let token = TokenSummary {
        id,
        name,
        read_only: input.read_only,
        areas: chosen.map(|mut chosen| {
            sort_areas(&mut chosen);
            chosen
        }),
        expires_at: input.expires_at,
        last_used_at: None,
        created_at: now,
    };
    Ok(CreatedToken { token, value }.into())
}

pub fn list_tokens(core: &Core, principal: &Principal, input: TokensOf) -> OpResult<TokenList> {
    let db = core.db();
    let owner = owner(&db, principal, input.agent_id.as_deref())?;
    let (owner_column, owner_id) = owner.column();
    let mut statement = db.prepare(&format!(
        "SELECT id, name, read_only, all_areas, expires_at, last_used_at, created_at
         FROM tokens WHERE {owner_column} = ?1 ORDER BY created_at DESC, id"
    ))?;
    let rows = statement.query_map([owner_id], |row| {
        Ok((
            TokenSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                read_only: row.get(2)?,
                areas: None,
                expires_at: row.get(4)?,
                last_used_at: row.get(5)?,
                created_at: row.get(6)?,
            },
            row.get::<_, bool>(3)?,
        ))
    })?;
    let mut tokens = Vec::new();
    for row in rows {
        let (mut token, all_areas) = row?;
        if !all_areas {
            // The names of Areas the owner can no longer read stay hidden.
            let mut shown = Vec::new();
            for area_id in token_area_ids(&db, &token.id)? {
                if let Ok(area) = areas::readable_area(&db, principal, &area_id) {
                    shown.push(TokenArea { id: area.id, name: area.name });
                }
            }
            sort_areas(&mut shown);
            token.areas = Some(shown);
        }
        tokens.push(token);
    }
    Ok(TokenList { tokens }.into())
}

pub fn revoke_token(core: &Core, principal: &Principal, input: TokenId) -> OpResult<()> {
    let db = core.db();
    let owner = owner(&db, principal, input.agent_id.as_deref())?;
    let (owner_column, owner_id) = owner.column();
    let removed = db.execute(
        &format!("DELETE FROM tokens WHERE id = ?1 AND {owner_column} = ?2"),
        rusqlite::params![input.id, owner_id],
    )?;
    drop(db);
    if removed == 0 {
        return Err(OpError::NotFound("There is no such Token.".into()));
    }
    core.access_changed();
    Ok(().into())
}

/// Who a change by the caller is credited to: the Member, the Agent acting
/// for them through their Token, named after the Token, or a standalone Agent.
pub(crate) fn author_of(core: &Core, principal: &Principal) -> Result<crate::live::Author, OpError> {
    let (member_id, token) = match principal {
        Principal::Anonymous => return Err(OpError::NotPermitted("Sign in first.".into())),
        Principal::Agent { id, .. } => return Ok(crate::live::Author::Agent(id.clone())),
        Principal::Member { id, token, .. } => (id, token),
    };
    let Some(token) = token else { return Ok(crate::live::Author::Member(member_id.clone())) };
    let name: Option<String> = core
        .db()
        .query_row("SELECT name FROM tokens WHERE id = ?1", [&token.id], |row| row.get(0))
        .optional()?;
    // The Token was revoked a moment ago; the caller is still an Agent.
    let agent = name.unwrap_or_else(|| "Agent".into());
    Ok(crate::live::Author::AgentForMember { member_id: member_id.clone(), agent })
}

/// Finds out who acts through a Token value, and with which limits: the
/// Member who owns it, or the standalone Agent it is for. The value is the
/// Token's own, or an access token of a connection an app was allowed
/// (`connections`), which works for an hour. An unknown, expired or revoked
/// Token, or one whose owner was deactivated, is nobody. A use is
/// remembered as the Token's last, to the minute.
pub fn principal_for_token(core: &Core, value: &str) -> Result<Option<Principal>, OpError> {
    if !value.starts_with(VALUE_PREFIX) {
        return Ok(None);
    }
    let now = core.now();
    let db = core.db();
    let found = db
        .query_row(
            "SELECT tokens.id, tokens.read_only, tokens.all_areas, members.id, members.workspace_admin, agents.id
             FROM tokens
             LEFT JOIN members ON members.id = tokens.member_id AND members.deactivated_at IS NULL
             LEFT JOIN agents ON agents.id = tokens.agent_id AND agents.deactivated_at IS NULL
             WHERE tokens.id IN (
                     SELECT id FROM tokens WHERE secret_digest = ?1
                     UNION ALL
                     SELECT token_id FROM access_tokens WHERE digest = ?1 AND expires_at > ?2
                 )
               AND (tokens.expires_at IS NULL OR tokens.expires_at > ?2)
               AND (members.id IS NOT NULL OR agents.id IS NOT NULL)",
            rusqlite::params![digest(value), now],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, bool>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<bool>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .optional()?;
    let Some((token_id, read_only, all_areas, member_id, workspace_admin, agent_id)) = found else {
        return Ok(None);
    };
    let areas = if all_areas { None } else { Some(token_area_ids(&db, &token_id)?) };
    // An open editor checks its Token on every change, so a use is written
    // at most once a minute rather than each time.
    db.execute(
        "UPDATE tokens SET last_used_at = ?1
         WHERE id = ?2 AND (last_used_at IS NULL OR last_used_at <= ?1 - ?3)",
        rusqlite::params![now, token_id, LAST_USE_PRECISION],
    )?;
    let token = TokenLimits { id: token_id, read_only, areas };
    Ok(Some(match (member_id, agent_id) {
        (Some(id), _) => Principal::Member { id, workspace_admin: workspace_admin.unwrap_or(false), session: None, token: Some(token) },
        (None, Some(id)) => Principal::Agent { id, token },
        (None, None) => return Ok(None),
    }))
}

fn token_area_ids(db: &Connection, token_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut statement = db.prepare("SELECT area_id FROM token_areas WHERE token_id = ?1")?;
    let ids = statement.query_map([token_id], |row| row.get(0))?.collect();
    ids
}

fn sort_areas(areas: &mut [TokenArea]) {
    areas.sort_by(|a, b| (a.name.to_lowercase(), &a.id).cmp(&(b.name.to_lowercase(), &b.id)));
}

fn valid_name(name: &str) -> Result<String, OpError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(OpError::InvalidInput("Enter a name for the Token.".into()));
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return Err(OpError::InvalidInput(format!(
            "Choose a name of at most {MAX_NAME_LENGTH} characters."
        )));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::accounts::{get_session, Nothing};
    use crate::ops::areas::{create_area, list_areas, NewArea};
    use crate::ops::members::list_members;
    use crate::ops::permissions::Role;
    use crate::ops::testing::{fresh, give_role, member};
    use crate::ops::tree::{create_page, get_page, get_tree, page_to_change, NewEntry, PageId, TreeOfArea};
    use serde_json::json;

    fn named(name: &str) -> NewToken {
        NewToken { name: name.into(), read_only: false, area_ids: None, expires_at: None, agent_id: None }
    }

    #[test]
    fn a_member_creates_a_token_that_acts_as_them() {
        let core = fresh();
        let grace = member(&core, "Grace", false);

        let created = create_token(&core, &grace, named("Claude on my laptop")).unwrap().output;

        assert!(created.value.starts_with("turtle_"), "{}", created.value);
        assert_eq!(created.token.name, "Claude on my laptop");
        assert!(!created.token.read_only);
        assert_eq!(created.token.areas, None);
        let agent = principal_for_token(&core, &created.value).unwrap().expect("the Token is accepted");
        let session = get_session(&core, &agent, Nothing {}).unwrap().output;
        assert_eq!(session.member.unwrap().name, "Grace");
    }

    #[test]
    fn a_tokens_value_is_shown_only_when_it_is_created() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let created = create_token(&core, &grace, named("Agent")).unwrap().output;

        let listed = list_tokens(&core, &grace, TokensOf::default()).unwrap().output;
        assert_eq!(listed.tokens, [created.token]);
        let listed = serde_json::to_string(&listed).unwrap();
        assert!(!listed.contains(&created.value));
    }

    #[test]
    fn an_unknown_value_is_nobody() {
        let core = fresh();
        assert_eq!(principal_for_token(&core, "turtle_made-up").unwrap(), None);
    }

    fn agent(core: &Core, owner: &Principal, input: NewToken) -> (CreatedToken, Principal) {
        let created = create_token(core, owner, input).unwrap().output;
        let agent = principal_for_token(core, &created.value).unwrap().unwrap();
        (created, agent)
    }

    #[test]
    fn a_member_sees_when_each_token_was_last_used_and_revokes_one() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let (kept, _) = agent(&core, &grace, named("Kept"));
        core.advance(60);
        let (revoked, _) = agent(&core, &grace, named("Revoked"));
        core.advance(60);
        principal_for_token(&core, &kept.value).unwrap().unwrap();

        let listed = list_tokens(&core, &grace, TokensOf::default()).unwrap().output.tokens;
        let seen: Vec<_> = listed.iter().map(|token| (token.name.as_str(), token.last_used_at)).collect();
        assert_eq!(seen, [("Revoked", Some(1_700_000_060)), ("Kept", Some(1_700_000_120))]);

        revoke_token(&core, &grace, TokenId { id: revoked.token.id.clone(), agent_id: None }).unwrap();
        assert_eq!(principal_for_token(&core, &revoked.value).unwrap(), None);
        assert!(principal_for_token(&core, &kept.value).unwrap().is_some());
        let names: Vec<_> = list_tokens(&core, &grace, TokensOf::default()).unwrap().output.tokens.into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["Kept"]);
    }

    #[test]
    fn nobody_revokes_or_sees_another_members_tokens() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", true);
        let (created, _) = agent(&core, &grace, named("Agent"));

        assert!(list_tokens(&core, &ken, TokensOf::default()).unwrap().output.tokens.is_empty());
        let refused = revoke_token(&core, &ken, TokenId { id: created.token.id.clone(), agent_id: None }).unwrap_err();
        assert_eq!(refused.kind(), "not_found");
        assert!(principal_for_token(&core, &created.value).unwrap().is_some());
    }

    #[test]
    fn a_token_stops_working_when_it_expires() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let input = NewToken { expires_at: Some(1_700_000_000 + 3600), ..named("For an hour") };
        let (created, _) = agent(&core, &grace, input);

        core.advance(3600);
        assert_eq!(principal_for_token(&core, &created.value).unwrap(), None);

        let past = NewToken { expires_at: Some(core.now()), ..named("Already over") };
        assert_eq!(create_token(&core, &grace, past).unwrap_err().kind(), "invalid_input");
    }

    #[test]
    fn deactivating_a_member_disables_their_tokens() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let grace = member(&core, "Grace", false);
        let (created, _) = agent(&core, &grace, named("Agent"));

        crate::ops::members::deactivate_member(&core, &ada, crate::ops::members::MemberId { id: "member-grace".into() }).unwrap();

        assert_eq!(principal_for_token(&core, &created.value).unwrap(), None);
    }

    #[test]
    fn a_token_needs_a_name() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        assert_eq!(create_token(&core, &grace, named("  ")).unwrap_err().kind(), "invalid_input");
        assert_eq!(create_token(&core, &Principal::Anonymous, named("Agent")).unwrap_err().kind(), "not_permitted");
    }

    fn area(core: &Core, admin: &Principal, name: &str) -> String {
        create_area(core, admin, NewArea { name: name.into() }).unwrap().output.id
    }

    fn area_names(core: &Core, principal: &Principal) -> Vec<String> {
        let list = list_areas(core, principal, Nothing {}).unwrap().output;
        list.areas.into_iter().map(|area| area.name).collect()
    }

    #[test]
    fn a_token_limited_to_areas_sees_no_others() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let engineering = area(&core, &grace, "Engineering");
        let secret = area(&core, &grace, "Secret");
        let input = NewToken { area_ids: Some(vec![engineering.clone()]), ..named("Engineering only") };
        let (created, agent) = agent(&core, &grace, input);

        let shown = created.token.areas.unwrap();
        assert_eq!(shown, [TokenArea { id: engineering.clone(), name: "Engineering".into() }]);
        assert_eq!(area_names(&core, &agent), ["Engineering"]);
        let hidden = get_tree(&core, &agent, TreeOfArea { area_id: secret.clone() }).unwrap_err();
        let missing = get_tree(&core, &agent, TreeOfArea { area_id: "no-such-area".into() }).unwrap_err();
        assert_eq!(hidden.kind(), "not_found");
        assert_eq!(hidden.message(), missing.message());
        let page = NewEntry { area_id: secret, parent_id: None, title: "Leak".into() };
        assert_eq!(create_page(&core, &agent, page.into()).unwrap_err().kind(), "not_found");
        let elsewhere = create_area(&core, &agent, NewArea { name: "Elsewhere".into() }).unwrap_err();
        assert_eq!(elsewhere.kind(), "not_permitted");
    }

    #[test]
    fn a_token_can_only_be_limited_to_areas_its_owner_can_read() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let linus = member(&core, "Linus", false);
        let kernel = area(&core, &linus, "Kernel");

        let input = NewToken { area_ids: Some(vec![kernel]), ..named("Sneaky") };
        assert_eq!(create_token(&core, &grace, input).unwrap_err().kind(), "not_found");
        let input = NewToken { area_ids: Some(vec![]), ..named("Nothing") };
        assert_eq!(create_token(&core, &grace, input).unwrap_err().kind(), "invalid_input");
    }

    #[test]
    fn a_token_never_has_more_access_than_its_owner_has_now() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ken = member(&core, "Ken", false);
        let engineering = area(&core, &grace, "Engineering");
        give_role(&core, &engineering, &ken, Role::Read);
        let input = NewToken { area_ids: Some(vec![engineering.clone()]), ..named("Agent") };
        let (_, agent) = agent(&core, &ken, input);

        let page = NewEntry { area_id: engineering.clone(), parent_id: None, title: "Notes".into() };
        assert_eq!(create_page(&core, &agent, page.into()).unwrap_err().kind(), "not_permitted");

        let holder = crate::ops::grants::GrantHolder::Member { member_id: "member-ken".into() };
        crate::ops::grants::remove_grant(&core, &grace, crate::ops::grants::RemoveGrant { area_id: engineering, holder }).unwrap();
        assert!(area_names(&core, &agent).is_empty());
        let listed = list_tokens(&core, &ken, TokensOf::default()).unwrap().output.tokens;
        assert_eq!(listed[0].areas, Some(vec![]));
    }

    #[test]
    fn a_read_only_token_reads_but_changes_nothing() {
        let core = fresh();
        let grace = member(&core, "Grace", true);
        let engineering = area(&core, &grace, "Engineering");
        let page = create_page(&core, &grace, NewEntry { area_id: engineering.clone(), parent_id: None, title: "Notes".into() }.into())
            .unwrap()
            .output;
        let (_, agent) = agent(&core, &grace, NewToken { read_only: true, ..named("Reader") });
        let catalogue = crate::ops::catalogue();

        let tree = catalogue.call(&core, &agent, "get_tree", json!({ "area_id": engineering })).unwrap();
        assert_eq!(tree.output["area"]["role"], "read");
        let opened = get_page(&core, &agent, PageId { id: page.id.clone() }).unwrap().output;
        assert_eq!(opened.role, Role::Read);
        assert_eq!(page_to_change(&core, &agent, &page.id).unwrap_err().kind(), "not_permitted");
        assert!(catalogue.call(&core, &agent, "list_members", json!({})).is_ok());
        for operation in catalogue.operations().iter().filter(|operation| operation.changes) {
            let refused = catalogue.call(&core, &agent, operation.name, json!({})).unwrap_err();
            assert_eq!(refused.kind(), "not_permitted", "{}", operation.name);
        }
    }

    #[test]
    fn a_token_reaches_no_browser_only_operation_so_it_cannot_make_tokens() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let (_, agent) = agent(&core, &grace, named("Agent"));
        let catalogue = crate::ops::catalogue();

        let made = catalogue.call(&core, &agent, "create_token", json!({ "name": "Wider" })).unwrap_err();
        assert_eq!(made.kind(), "not_permitted");
        for operation in catalogue.operations().iter().filter(|operation| operation.browser_only) {
            let refused = catalogue.call(&core, &agent, operation.name, json!({})).unwrap_err();
            assert_eq!(refused.kind(), "not_permitted", "{}", operation.name);
        }
    }

    fn standalone(core: &Core, admin: &Principal, name: &str) -> String {
        crate::ops::agents::create_agent(core, admin, crate::ops::agents::NewAgent { name: name.into() })
            .unwrap()
            .output
            .id
    }

    fn for_agent(agent_id: &str, name: &str) -> NewToken {
        NewToken { agent_id: Some(agent_id.into()), ..named(name) }
    }

    fn tokens_of(core: &Core, caller: &Principal, agent_id: Option<&str>) -> Vec<String> {
        let input = TokensOf { agent_id: agent_id.map(str::to_string) };
        list_tokens(core, caller, input).unwrap().output.tokens.into_iter().map(|token| token.name).collect()
    }

    #[test]
    fn a_workspace_admin_manages_the_tokens_of_a_standalone_agent() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let reviewer = standalone(&core, &ada, "Reviewer");

        let (kept, acting) = agent(&core, &ada, for_agent(&reviewer, "CI"));
        core.advance(60);
        let (revoked, _) = agent(&core, &ada, for_agent(&reviewer, "Laptop"));

        assert!(matches!(&acting, Principal::Agent { id, .. } if *id == reviewer));
        assert_eq!(tokens_of(&core, &ada, Some(&reviewer)), ["Laptop", "CI"]);
        assert!(tokens_of(&core, &ada, None).is_empty(), "the Agent's Tokens are not Ada's");
        let revoke = TokenId { id: revoked.token.id.clone(), agent_id: Some(reviewer.clone()) };
        revoke_token(&core, &ada, revoke).unwrap();
        assert_eq!(principal_for_token(&core, &revoked.value).unwrap(), None);
        assert!(principal_for_token(&core, &kept.value).unwrap().is_some());
        // A Member's own revoking does not reach the Agent's Tokens.
        let own = TokenId { id: kept.token.id.clone(), agent_id: None };
        assert_eq!(revoke_token(&core, &ada, own).unwrap_err().kind(), "not_found");
    }

    #[test]
    fn only_a_workspace_admin_manages_an_agents_tokens() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let grace = member(&core, "Grace", false);
        let reviewer = standalone(&core, &ada, "Reviewer");
        let (created, _) = agent(&core, &ada, for_agent(&reviewer, "CI"));

        assert_eq!(create_token(&core, &grace, for_agent(&reviewer, "Mine")).unwrap_err().kind(), "not_permitted");
        let input = TokensOf { agent_id: Some(reviewer.clone()) };
        assert_eq!(list_tokens(&core, &grace, input).unwrap_err().kind(), "not_permitted");
        let revoke = TokenId { id: created.token.id, agent_id: Some(reviewer) };
        assert_eq!(revoke_token(&core, &grace, revoke).unwrap_err().kind(), "not_permitted");
        let unknown = for_agent("no-such-agent", "CI");
        assert_eq!(create_token(&core, &ada, unknown).unwrap_err().kind(), "not_found");
    }

    #[test]
    fn deactivating_an_agent_disables_its_tokens() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let reviewer = standalone(&core, &ada, "Reviewer");
        let (created, _) = agent(&core, &ada, for_agent(&reviewer, "CI"));

        let id = crate::ops::agents::AgentId { id: reviewer.clone() };
        crate::ops::agents::deactivate_agent(&core, &ada, id).unwrap();

        assert_eq!(principal_for_token(&core, &created.value).unwrap(), None);
        let refused = create_token(&core, &ada, for_agent(&reviewer, "Another")).unwrap_err();
        assert_eq!(refused.kind(), "conflict");
    }

    #[test]
    fn a_standalone_agent_reaches_only_what_it_was_given_and_never_manages_members() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let reviewer = standalone(&core, &ada, "Reviewer");
        let engineering = area(&core, &ada, "Engineering");
        let open = area(&core, &ada, "Open to all");
        area(&core, &ada, "Secret");
        let to_all = crate::ops::grants::GrantHolder::AllMembers;
        let grant = crate::ops::grants::SetGrant { area_id: open, holder: to_all, role: Role::Write };
        crate::ops::grants::set_grant(&core, &ada, grant).unwrap();
        let holder = crate::ops::grants::GrantHolder::Agent { agent_id: reviewer.clone() };
        let grant = crate::ops::grants::SetGrant { area_id: engineering.clone(), holder, role: Role::Write };
        crate::ops::grants::set_grant(&core, &ada, grant).unwrap();
        let (_, agent) = agent(&core, &ada, for_agent(&reviewer, "CI"));

        assert_eq!(area_names(&core, &agent), ["Engineering"]);
        let page = NewEntry { area_id: engineering, parent_id: None, title: "Review".into() };
        assert!(create_page(&core, &agent, page.into()).is_ok());
        assert_eq!(list_members(&core, &agent, Nothing {}).unwrap_err().kind(), "not_permitted");
        let elsewhere = create_area(&core, &agent, NewArea { name: "Elsewhere".into() }).unwrap_err();
        assert_eq!(elsewhere.kind(), "not_permitted");
        let session = get_session(&core, &agent, Nothing {}).unwrap().output;
        assert_eq!(session.member, None);
        assert_eq!(session.agent.map(|agent| agent.name), Some("Reviewer".into()));
    }

    #[test]
    fn an_agent_acting_for_a_member_reaches_their_personal_area_and_a_standalone_agent_never_does() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let grace = member(&core, "Grace", false);
        let personal = crate::ops::testing::personal_area(&core, &grace);
        let (_, everywhere) = agent(&core, &grace, named("Everywhere"));
        let (_, only_personal) = agent(&core, &grace, NewToken { area_ids: Some(vec![personal.clone()]), ..named("Notes") });
        let reviewer = standalone(&core, &ada, "Reviewer");
        let (_, standalone_agent) = agent(&core, &ada, for_agent(&reviewer, "CI"));

        for acting in [&everywhere, &only_personal] {
            assert_eq!(area_names(&core, acting), ["Personal"]);
            let page = NewEntry { area_id: personal.clone(), parent_id: None, title: "Quick thought".into() };
            assert!(create_page(&core, acting, page.into()).is_ok());
        }
        assert!(area_names(&core, &standalone_agent).is_empty());
        let hidden = get_tree(&core, &standalone_agent, TreeOfArea { area_id: personal }).unwrap_err();
        assert_eq!(hidden.kind(), "not_found");
    }

    #[test]
    fn a_workspace_admins_token_limited_to_areas_does_not_manage_members() {
        let core = fresh();
        let ada = member(&core, "Ada", true);
        let engineering = area(&core, &ada, "Engineering");
        let (_, everywhere) = agent(&core, &ada, named("Everywhere"));
        let (_, limited) = agent(&core, &ada, NewToken { area_ids: Some(vec![engineering]), ..named("Limited") });

        assert!(list_members(&core, &everywhere, Nothing {}).is_ok());
        assert_eq!(list_members(&core, &limited, Nothing {}).unwrap_err().kind(), "not_permitted");
    }
}
