//! Every operation of the catalogue, called by someone holding each Role on
//! an Area. A new operation fails this test until it has a row here.

use super::permissions::Role;
use super::testing::{fresh, give_role, member};
use super::{catalogue, Principal};
use serde_json::{json, Value};

/// Who calls: an Area Admin, Write or Read, a Member without a Role, and a
/// Workspace Admin without a Grant.
const COLUMNS: [&str; 5] = ["Admin", "Write", "Read", "no Role", "Workspace Admin"];

/// What exists before each call. Every call gets a fresh one.
struct Fixture {
    area_id: String,
    page_id: String,
    folder_id: String,
    /// A Member with Read, the subject of Member and Grant operations.
    ken_id: String,
}

/// The outcome expected from each column: "ok" or the kind of error.
type Row = (&'static str, fn(&Fixture) -> Value, [&'static str; 5]);

const OK: &str = "ok";
const HIDDEN: &str = "not_found";
const REFUSED: &str = "not_permitted";

fn table() -> Vec<Row> {
    vec![
        ("get_session", |_| json!({}), [OK; 5]),
        (
            "create_first_account",
            |_| json!({"name": "Eve", "email": "eve@example.org", "password": "long enough password"}),
            ["conflict"; 5],
        ),
        ("sign_in", |_| json!({"email": "nobody@example.org", "password": "long enough password"}), [REFUSED; 5]),
        ("sign_out", |_| json!({}), [OK; 5]),
        ("invite_member", |_| json!({"email": "new@example.org"}), [REFUSED, REFUSED, REFUSED, REFUSED, OK]),
        (
            "accept_invitation",
            |_| json!({"secret": "made-up", "name": "Eve", "password": "long enough password"}),
            [REFUSED; 5],
        ),
        ("list_members", |_| json!({}), [REFUSED, REFUSED, REFUSED, REFUSED, OK]),
        ("deactivate_member", |f| json!({"id": f.ken_id}), [REFUSED, REFUSED, REFUSED, REFUSED, OK]),
        (
            "set_workspace_admin",
            |f| json!({"id": f.ken_id, "workspace_admin": true}),
            [REFUSED, REFUSED, REFUSED, REFUSED, OK],
        ),
        ("create_password_reset", |f| json!({"id": f.ken_id}), [REFUSED, REFUSED, REFUSED, REFUSED, OK]),
        ("reset_password", |_| json!({"secret": "made-up", "password": "long enough password"}), [REFUSED; 5]),
        ("create_area", |_| json!({"name": "Sales"}), [OK; 5]),
        ("get_area", |f| json!({"id": f.area_id}), [OK, OK, OK, HIDDEN, OK]),
        ("list_areas", |_| json!({}), [OK; 5]),
        ("rename_area", |f| json!({"id": f.area_id, "name": "Renamed"}), [OK, REFUSED, REFUSED, HIDDEN, OK]),
        ("create_page", |f| json!({"area_id": f.area_id, "title": "New"}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("create_folder", |f| json!({"area_id": f.area_id, "title": "New"}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("rename_page", |f| json!({"id": f.page_id, "title": "Renamed"}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("rename_folder", |f| json!({"id": f.folder_id, "title": "Renamed"}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("get_page", |f| json!({"id": f.page_id}), [OK, OK, OK, HIDDEN, OK]),
        ("get_tree", |f| json!({"area_id": f.area_id}), [OK, OK, OK, HIDDEN, OK]),
        ("list_grants", |f| json!({"area_id": f.area_id}), [OK, REFUSED, REFUSED, HIDDEN, OK]),
        (
            "set_grant",
            |f| json!({"area_id": f.area_id, "holder": {"kind": "member", "member_id": f.ken_id}, "role": "write"}),
            [OK, REFUSED, REFUSED, HIDDEN, OK],
        ),
        (
            "remove_grant",
            |f| json!({"area_id": f.area_id, "holder": {"kind": "member", "member_id": f.ken_id}}),
            [OK, REFUSED, REFUSED, HIDDEN, OK],
        ),
    ]
}

#[test]
fn every_operation_respects_the_role_of_its_caller() {
    let catalogue = catalogue();
    let table = table();

    let mut listed: Vec<_> = table.iter().map(|(name, _, _)| *name).collect();
    let mut known: Vec<_> = catalogue.operations().iter().map(|op| op.name).collect();
    listed.sort_unstable();
    known.sort_unstable();
    assert_eq!(listed, known, "every operation needs exactly one row");

    for (name, input, expected) in table {
        for (column, expected) in COLUMNS.into_iter().zip(expected) {
            let core = fresh();
            let grace = member(&core, "Grace", false);
            let ken = member(&core, "Ken", false);
            let area = catalogue.call(&core, &grace, "create_area", json!({"name": "Engineering"})).unwrap().output;
            let area_id = area["id"].as_str().unwrap().to_string();
            let entry = |operation, title| {
                let created = catalogue
                    .call(&core, &grace, operation, json!({"area_id": area_id, "title": title}))
                    .unwrap()
                    .output;
                created["id"].as_str().unwrap().to_string()
            };
            let fixture = Fixture {
                page_id: entry("create_page", "Notes"),
                folder_id: entry("create_folder", "Drafts"),
                area_id: area_id.clone(),
                ken_id: match &ken {
                    Principal::Member { id, .. } => id.clone(),
                    Principal::Anonymous => unreachable!(),
                },
            };
            give_role(&core, &area_id, &ken, Role::Read);
            let caller = member(&core, "Caller", column == "Workspace Admin");
            let role = match column {
                "Admin" => Some(Role::Admin),
                "Write" => Some(Role::Write),
                "Read" => Some(Role::Read),
                _ => None,
            };
            if let Some(role) = role {
                give_role(&core, &area_id, &caller, role);
            }

            let outcome = match catalogue.call(&core, &caller, name, input(&fixture)) {
                Ok(_) => OK,
                Err(error) => error.kind(),
            };
            assert_eq!(outcome, expected, "{name} called with {column}");
        }
    }
}
