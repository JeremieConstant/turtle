//! Areas: creating them, listing the ones a principal can read, renaming them.

use super::accounts::{random_hex, Nothing};
use super::permissions::{self, Holder, Role};
use super::{Catalogue, Core, OpError, OpResult, Principal};
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(super) fn register(catalogue: &mut Catalogue) {
    catalogue.add(
        "create_area",
        "Creates an Area. The caller becomes its Admin and is the only one with access. A standalone Agent, and a Token limited to chosen Areas, cannot.",
        create_area,
    );
    catalogue
        .add(
            "get_area",
            "Gets one Area by its ID. An Area the caller cannot read is reported as not found.",
            get_area,
        )
        .changes = false;
    catalogue
        .add(
            "list_areas",
            "Lists the Areas the caller can read: their own Personal Area first, then the others by name.",
            list_areas,
        )
        .changes = false;
    catalogue.add(
        "rename_area",
        "Renames an Area. Needs the Admin Role on it. A Personal Area cannot be renamed.",
        rename_area,
    );
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Area {
    pub id: String,
    pub name: String,
    /// The Role the caller holds on the Area.
    pub role: Role,
    /// Whether this is the caller's Personal Area: theirs alone, never
    /// granted, renamed or deleted. Nobody else ever sees it.
    pub personal: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AreaList {
    pub areas: Vec<Area>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewArea {
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AreaId {
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RenameArea {
    pub id: String,
    pub name: String,
}

/// Long enough for any sensible name, short enough to fit a list.
const MAX_NAME_LENGTH: usize = 200;

pub fn create_area(core: &Core, principal: &Principal, input: NewArea) -> OpResult<Area> {
    // An Area whose only Admin is an Agent would have nobody to manage it.
    let member_id = signed_in_member(
        principal,
        "An Agent cannot create an Area. Ask an Admin of an Area to give the Agent a Role there.",
    )?;
    if principal.token().is_some_and(|token| token.areas.is_some()) {
        return Err(OpError::NotPermitted(
            "This Token is limited to chosen Areas, so it cannot create a new one.".into(),
        ));
    }
    let name = valid_name(&input.name)?;
    let area_id = random_hex(16)?;
    let root_id = random_hex(16)?;
    let now = core.now();

    let mut db = core.db();
    let transaction = db.transaction()?;
    transaction.execute(
        "INSERT INTO areas (id, name, created_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![area_id, name, now],
    )?;
    transaction.execute(
        "INSERT INTO nodes (id, area_id, parent_id, kind, created_at) VALUES (?1, ?2, NULL, 'root', ?3)",
        rusqlite::params![root_id, area_id, now],
    )?;
    // A new Area is private: its creator's Grant is the only one.
    permissions::grant(&transaction, &root_id, &Holder::Member(member_id.to_string()), Role::Admin)?;
    transaction.commit()?;

    Ok(Area {
        id: area_id,
        name,
        role: Role::Admin,
        personal: false,
    }
    .into())
}

/// What every Personal Area is called.
const PERSONAL_AREA_NAME: &str = "Personal";

/// Gives a new Member their Personal Area (ADR-0008). It carries no Grants:
/// `permissions` lets its Member in and nobody else.
pub(super) fn create_personal_area(db: &Connection, member_id: &str, now: i64) -> Result<(), OpError> {
    let area_id = random_hex(16)?;
    db.execute(
        "INSERT INTO areas (id, name, created_at, personal_member_id) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![area_id, PERSONAL_AREA_NAME, now, member_id],
    )?;
    db.execute(
        "INSERT INTO nodes (id, area_id, parent_id, kind, created_at) VALUES (?1, ?2, NULL, 'root', ?3)",
        rusqlite::params![random_hex(16)?, area_id, now],
    )?;
    Ok(())
}

pub fn get_area(core: &Core, principal: &Principal, input: AreaId) -> OpResult<Area> {
    signed_in(principal)?;
    Ok(readable_area(&core.db(), principal, &input.id)?.into())
}

pub fn rename_area(core: &Core, principal: &Principal, input: RenameArea) -> OpResult<Area> {
    signed_in(principal)?;
    let db = core.db();
    // Access is checked before the input, so an unreadable Area stays hidden
    // whatever name is sent.
    let area = administered_area(&db, principal, &input.id, "Only an Admin of this Area can rename it.")?;
    let name = valid_name(&input.name)?;
    db.execute(
        "UPDATE areas SET name = ?1 WHERE id = ?2",
        rusqlite::params![name, area.id],
    )?;
    Ok(Area { name, ..area }.into())
}

/// An Area the principal is Admin of, to rename it or manage its Grants. One
/// they cannot read is not found; one they can read without being Admin is
/// refused with `refusal`. Their own Personal Area is refused too: it can be
/// neither renamed nor granted.
pub(super) fn administered_area(
    db: &Connection,
    principal: &Principal,
    id: &str,
    refusal: &str,
) -> Result<Area, OpError> {
    let area = readable_area(db, principal, id)?;
    if area.role < Role::Admin {
        return Err(OpError::NotPermitted(refusal.into()));
    }
    if area.personal {
        return Err(OpError::NotPermitted(
            "Your Personal Area is yours alone: it keeps its name, and nobody else can be given a Role on it. To share Pages, create another Area.".into(),
        ));
    }
    Ok(area)
}

/// An Area the principal can read. One they cannot read is reported exactly
/// like one that does not exist, so its existence is not revealed.
pub(super) fn readable_area(db: &Connection, principal: &Principal, id: &str) -> Result<Area, OpError> {
    let not_found = || OpError::NotFound("There is no such Area.".into());
    let found = db
        .query_row(
            "SELECT areas.name, nodes.id, areas.personal_member_id IS NOT NULL
             FROM areas JOIN nodes ON nodes.area_id = areas.id AND nodes.parent_id IS NULL
             WHERE areas.id = ?1",
            [id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, bool>(2)?)),
        )
        .optional()?;
    let (name, root_id, personal) = found.ok_or_else(not_found)?;
    let role = permissions::role_on(db, principal, &root_id)?.ok_or_else(not_found)?;
    Ok(Area {
        id: id.to_string(),
        name,
        role,
        personal,
    })
}

/// The principal's own Personal Area, or `None` when they have none they can
/// reach: a standalone Agent, or a Token limited to other Areas.
pub(super) fn personal_area_of(db: &Connection, principal: &Principal) -> Result<Option<Area>, OpError> {
    let Principal::Member { id, .. } = principal else { return Ok(None) };
    let area_id: Option<String> = db
        .query_row("SELECT id FROM areas WHERE personal_member_id = ?1", [id], |row| row.get(0))
        .optional()?;
    let Some(area_id) = area_id else { return Ok(None) };
    match readable_area(db, principal, &area_id) {
        Ok(area) => Ok(Some(area)),
        Err(OpError::NotFound(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn list_areas(core: &Core, principal: &Principal, _: Nothing) -> OpResult<AreaList> {
    signed_in(principal)?;
    let db = core.db();
    let mut statement = db.prepare(
        "SELECT areas.id, areas.name, nodes.id, areas.personal_member_id IS NOT NULL
         FROM areas JOIN nodes ON nodes.area_id = areas.id AND nodes.parent_id IS NULL
         ORDER BY areas.personal_member_id IS NULL, areas.name COLLATE NOCASE, areas.id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, bool>(3)?))
    })?;
    let mut areas = Vec::new();
    for row in rows {
        let (id, name, root_id, personal) = row?;
        if let Some(role) = permissions::role_on(&db, principal, &root_id)? {
            areas.push(Area { id, name, role, personal });
        }
    }
    Ok(AreaList { areas }.into())
}

/// Refuses nobody signed in: only Members and Agents work with Areas and Pages.
pub fn signed_in(principal: &Principal) -> Result<(), OpError> {
    match principal {
        Principal::Member { .. } | Principal::Agent { .. } => Ok(()),
        Principal::Anonymous => Err(OpError::NotPermitted("Sign in first.".into())),
    }
}

/// The acting Member's ID, for what only a Member can do, also through a
/// Token: a standalone Agent has no account of its own to do it with.
pub fn signed_in_member<'a>(principal: &'a Principal, refusal: &str) -> Result<&'a str, OpError> {
    match principal {
        Principal::Member { id, .. } => Ok(id),
        Principal::Agent { .. } => Err(OpError::NotPermitted(refusal.into())),
        Principal::Anonymous => Err(OpError::NotPermitted("Sign in first.".into())),
    }
}

fn valid_name(name: &str) -> Result<String, OpError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(OpError::InvalidInput("Enter a name for the Area.".into()));
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
    use crate::ops::testing::{fresh, give_role, member, personal_area};

    fn named(name: &str) -> NewArea {
        NewArea { name: name.into() }
    }

    /// The Areas listed for the principal, leaving out their Personal Area.
    fn shared(core: &Core, principal: &Principal) -> Vec<Area> {
        let list = list_areas(core, principal, Nothing {}).unwrap().output;
        list.areas.into_iter().filter(|area| !area.personal).collect()
    }

    fn names(core: &Core, principal: &Principal) -> Vec<String> {
        shared(core, principal).into_iter().map(|area| area.name).collect()
    }

    #[test]
    fn a_member_creates_an_area_and_becomes_its_admin() {
        let core = fresh();
        let grace = member(&core, "Grace", false);

        let area = create_area(&core, &grace, named("Engineering")).unwrap().output;

        assert_eq!(area.name, "Engineering");
        assert_eq!(area.role, Role::Admin);
        assert_eq!(names(&core, &grace), ["Engineering"]);
    }

    #[test]
    fn a_new_area_is_invisible_to_other_members() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let linus = member(&core, "Linus", false);

        create_area(&core, &grace, named("Secret project")).unwrap();
        create_area(&core, &linus, named("Kernel")).unwrap();

        assert_eq!(names(&core, &grace), ["Secret project"]);
        assert_eq!(names(&core, &linus), ["Kernel"]);
    }

    #[test]
    fn a_workspace_admin_sees_every_area_as_its_admin() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ada = member(&core, "Ada", true);
        create_area(&core, &grace, named("Secret project")).unwrap();
        create_area(&core, &grace, named("Handbook")).unwrap();

        let list = shared(&core, &ada);

        let seen: Vec<_> = list.iter().map(|area| (area.name.as_str(), area.role)).collect();
        assert_eq!(seen, [("Handbook", Role::Admin), ("Secret project", Role::Admin)]);
    }

    #[test]
    fn an_area_one_can_read_is_found_by_its_id() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let ada = member(&core, "Ada", true);
        let created = create_area(&core, &grace, named("Engineering")).unwrap().output;

        let id = AreaId { id: created.id.clone() };
        assert_eq!(get_area(&core, &grace, id).unwrap().output, created);
        let id = AreaId { id: created.id.clone() };
        assert_eq!(get_area(&core, &ada, id).unwrap().output.role, Role::Admin);
    }

    #[test]
    fn an_area_one_cannot_read_answers_like_one_that_does_not_exist() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let linus = member(&core, "Linus", false);
        let secret = create_area(&core, &grace, named("Secret project")).unwrap().output;

        let hidden = get_area(&core, &linus, AreaId { id: secret.id }).unwrap_err();
        let missing = get_area(&core, &linus, AreaId { id: "no-such-area".into() }).unwrap_err();

        assert_eq!(hidden.kind(), "not_found");
        assert_eq!(hidden.kind(), missing.kind());
        assert_eq!(hidden.message(), missing.message());
        assert!(!hidden.message().contains("Secret"));
    }

    #[test]
    fn a_member_with_a_grant_sees_the_area_with_that_role() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let linus = member(&core, "Linus", false);
        let area = create_area(&core, &grace, named("Engineering")).unwrap().output;

        give_role(&core, &area.id, &linus, Role::Read);

        assert_eq!(shared(&core, &linus), [Area { role: Role::Read, ..area }]);
    }

    #[test]
    fn only_an_admin_of_the_area_can_rename_it() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = create_area(&core, &grace, named("Engineering")).unwrap().output;
        let ada = member(&core, "Ada", true);
        let admin = member(&core, "Barbara", false);
        let writer = member(&core, "Ken", false);
        let reader = member(&core, "Dennis", false);
        let outsider = member(&core, "Linus", false);
        give_role(&core, &area.id, &admin, Role::Admin);
        give_role(&core, &area.id, &writer, Role::Write);
        give_role(&core, &area.id, &reader, Role::Read);

        let table = [
            (&grace, Ok(())),
            (&ada, Ok(())),
            (&admin, Ok(())),
            (&writer, Err("not_permitted")),
            (&reader, Err("not_permitted")),
            (&outsider, Err("not_found")),
            (&Principal::Anonymous, Err("not_permitted")),
        ];
        for (index, (principal, expected)) in table.into_iter().enumerate() {
            let name = format!("Engineering {index}");
            let input = RenameArea { id: area.id.clone(), name: name.clone() };
            let outcome = rename_area(&core, principal, input);
            match expected {
                Ok(()) => assert_eq!(outcome.unwrap().output.name, name, "{principal:?}"),
                Err(kind) => assert_eq!(outcome.unwrap_err().kind(), kind, "{principal:?}"),
            }
        }
        let current = get_area(&core, &grace, AreaId { id: area.id }).unwrap().output;
        assert_eq!(current.name, "Engineering 2");
    }

    #[test]
    fn a_rename_needs_a_valid_name() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let area = create_area(&core, &grace, named("Engineering")).unwrap().output;

        let refused = rename_area(&core, &grace, RenameArea { id: area.id.clone(), name: " ".into() });
        assert_eq!(refused.unwrap_err().kind(), "invalid_input");
        assert_eq!(names(&core, &grace), ["Engineering"]);
    }

    fn listed(core: &Core, principal: &Principal) -> Vec<(String, bool, Role)> {
        let list = list_areas(core, principal, Nothing {}).unwrap().output;
        list.areas.into_iter().map(|area| (area.name, area.personal, area.role)).collect()
    }

    #[test]
    fn the_first_account_comes_with_a_personal_area_listed_before_the_others() {
        let core = fresh();
        let account = crate::ops::accounts::NewAccount {
            name: "Ada".into(),
            email: "ada@example.org".into(),
            password: "a long enough password".into(),
        };
        let created = crate::ops::accounts::create_first_account(&core, &Principal::Anonymous, account).unwrap();
        let ada = Principal::Member { id: created.output.id, workspace_admin: true, session: None, token: None };
        create_area(&core, &ada, named("Engineering")).unwrap();
        create_area(&core, &ada, named("Archive")).unwrap();

        assert_eq!(
            listed(&core, &ada),
            [
                ("Personal".into(), true, Role::Admin),
                ("Archive".into(), false, Role::Admin),
                ("Engineering".into(), false, Role::Admin),
            ]
        );
    }

    #[test]
    fn a_personal_area_is_seen_by_its_member_alone_not_even_by_a_workspace_admin() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let linus = member(&core, "Linus", false);
        let ada = member(&core, "Ada", true);
        let personal = personal_area(&core, &grace);

        let seen = get_area(&core, &grace, AreaId { id: personal.clone() }).unwrap().output;
        assert_eq!(seen, Area { id: personal.clone(), name: "Personal".into(), role: Role::Admin, personal: true });
        for other in [&linus, &ada] {
            assert_eq!(listed(&core, other), [("Personal".into(), true, Role::Admin)], "{other:?}");
            let hidden = get_area(&core, other, AreaId { id: personal.clone() }).unwrap_err();
            let missing = get_area(&core, other, AreaId { id: "no-such-area".into() }).unwrap_err();
            assert_eq!((hidden.kind(), hidden.message()), (missing.kind(), missing.message()), "{other:?}");
        }
    }

    #[test]
    fn nobody_signed_in_can_create_or_list_areas() {
        let core = fresh();
        let refused = create_area(&core, &Principal::Anonymous, named("Engineering")).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
        let refused = list_areas(&core, &Principal::Anonymous, Nothing {}).unwrap_err();
        assert_eq!(refused.kind(), "not_permitted");
    }

    #[test]
    fn an_area_needs_a_name_of_sensible_length() {
        let core = fresh();
        let grace = member(&core, "Grace", false);
        let too_long = "x".repeat(MAX_NAME_LENGTH + 1);
        for name in ["", "   ", too_long.as_str()] {
            let refused = create_area(&core, &grace, named(name)).unwrap_err();
            assert_eq!(refused.kind(), "invalid_input", "{name:?}");
        }
        assert!(names(&core, &grace).is_empty());

        let area = create_area(&core, &grace, named("  Engineering  ")).unwrap().output;
        assert_eq!(area.name, "Engineering");
    }
}
