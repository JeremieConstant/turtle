//! Every operation of the catalogue, called by someone holding each Role on
//! an Area. A new operation fails this test until it has a row here.

use super::permissions::Role;
use super::testing::{fresh, give_role, member, personal_area};
use super::{catalogue, Principal};
use serde_json::{json, Value};

/// Who calls: an Area Admin, Write or Read, a Member without a Role, and a
/// Workspace Admin without a Grant.
const COLUMNS: [&str; 5] = ["Admin", "Write", "Read", "no Role", "Workspace Admin"];

/// What exists before each call. Every call gets a fresh one. The Page has
/// one Version.
struct Fixture {
    area_id: String,
    page_id: String,
    folder_id: String,
    /// A Page in the Trash.
    trashed_id: String,
    /// A Member with Read, the subject of Member and Grant operations.
    ken_id: String,
    /// A standalone Agent without a Role, the subject of Agent operations.
    agent_id: String,
    /// An Attachment of the Page.
    attachment_id: String,
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
        ("create_agent", |_| json!({"name": "Reviewer"}), [REFUSED, REFUSED, REFUSED, REFUSED, OK]),
        ("list_agents", |_| json!({}), [REFUSED, REFUSED, REFUSED, REFUSED, OK]),
        ("rename_agent", |f| json!({"id": f.agent_id, "name": "Proofreader"}), [REFUSED, REFUSED, REFUSED, REFUSED, OK]),
        ("deactivate_agent", |f| json!({"id": f.agent_id}), [REFUSED, REFUSED, REFUSED, REFUSED, OK]),
        ("create_area", |_| json!({"name": "Sales"}), [OK; 5]),
        ("get_area", |f| json!({"id": f.area_id}), [OK, OK, OK, HIDDEN, OK]),
        ("list_areas", |_| json!({}), [OK; 5]),
        ("rename_area", |f| json!({"id": f.area_id, "name": "Renamed"}), [OK, REFUSED, REFUSED, HIDDEN, OK]),
        ("create_page", |f| json!({"area_id": f.area_id, "title": "New"}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("create_folder", |f| json!({"area_id": f.area_id, "title": "New"}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("rename_page", |f| json!({"id": f.page_id, "title": "Renamed"}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("rename_folder", |f| json!({"id": f.folder_id, "title": "Renamed"}), [OK, OK, REFUSED, HIDDEN, OK]),
        (
            "move_entry",
            |f| json!({"id": f.page_id, "area_id": f.area_id, "before_id": f.folder_id}),
            [OK, OK, REFUSED, HIDDEN, OK],
        ),
        ("move_to_trash", |f| json!({"id": f.page_id}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("get_trash", |f| json!({"area_id": f.area_id}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("restore_from_trash", |f| json!({"id": f.trashed_id}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("delete_from_trash", |f| json!({"id": f.trashed_id}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("delete_area", |f| json!({"id": f.area_id}), [OK, REFUSED, REFUSED, HIDDEN, OK]),
        ("list_deleted_areas", |_| json!({}), [REFUSED, REFUSED, REFUSED, REFUSED, OK]),
        // The Area is not deleted.
        ("restore_area", |f| json!({"id": f.area_id}), [HIDDEN; 5]),
        // Written into the Area, a Quick Note takes the Write Role there, as any new Page does.
        ("create_quick_note", |f| json!({"title": "Idea", "area_id": f.area_id}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("list_quick_notes", |_| json!({}), [OK; 5]),
        ("get_page", |f| json!({"id": f.page_id}), [OK, OK, OK, HIDDEN, OK]),
        ("get_tree", |f| json!({"area_id": f.area_id}), [OK, OK, OK, HIDDEN, OK]),
        ("search", |f| json!({"query": "notes", "area_id": f.area_id}), [OK, OK, OK, HIDDEN, OK]),
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
        ("create_token", |f| json!({"name": "Agent", "area_ids": [f.area_id]}), [OK, OK, OK, HIDDEN, OK]),
        ("list_tokens", |_| json!({}), [OK; 5]),
        ("revoke_token", |_| json!({"id": "made-up"}), [HIDDEN; 5]),
        ("list_members_tokens", |_| json!({}), [REFUSED, REFUSED, REFUSED, REFUSED, OK]),
        ("describe_connecting", |_| json!({}), [OK; 5]),
        ("set_connecting_ai", |_| json!({"on": true}), [REFUSED, REFUSED, REFUSED, REFUSED, OK]),
        // This Turtle offers no OAuth, so no connection request exists.
        ("describe_connection_request", |_| json!({"client_id": "made-up"}), [HIDDEN; 5]),
        ("answer_connection_request", |_| json!({"client_id": "made-up", "allow": true}), [HIDDEN; 5]),
        ("list_versions", |f| json!({"page_id": f.page_id}), [OK, OK, OK, HIDDEN, OK]),
        ("get_version", |f| json!({"page_id": f.page_id, "number": 1}), [OK, OK, OK, HIDDEN, OK]),
        (
            "compare_versions",
            |f| json!({"page_id": f.page_id, "from": 1, "to": 1}),
            [OK, OK, OK, HIDDEN, OK],
        ),
        ("restore_version", |f| json!({"page_id": f.page_id, "number": 1}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("list_tags", |_| json!({}), [OK; 5]),
        ("add_tag", |f| json!({"page_id": f.page_id, "tag": "Runbook"}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("remove_tag", |f| json!({"page_id": f.page_id, "tag": "Runbook"}), [OK, OK, REFUSED, HIDDEN, OK]),
        // "hello" in base64.
        ("attach_file", |f| json!({"page_id": f.page_id, "name": "a.txt", "content": "aGVsbG8="}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("list_attachments", |f| json!({"id": f.page_id}), [OK, OK, OK, HIDDEN, OK]),
        ("read_attachment", |f| json!({"id": f.attachment_id}), [OK, OK, OK, HIDDEN, OK]),
        // It leaves out the Pages the caller cannot read instead of failing; ops::links tests which.
        ("get_page_titles", |_| json!({"page_ids": ["made-up"]}), [OK; 5]),
        ("get_page_markdown", |f| json!({"page_id": f.page_id}), [OK, OK, OK, HIDDEN, OK]),
        ("replace_page", |f| json!({"page_id": f.page_id, "markdown": "New."}), [OK, OK, REFUSED, HIDDEN, OK]),
        ("append_to_page", |f| json!({"page_id": f.page_id, "markdown": "More."}), [OK, OK, REFUSED, HIDDEN, OK]),
        // The Page has no headings, so those who may change it learn that.
        (
            "replace_section",
            |f| json!({"page_id": f.page_id, "heading": "Plan", "markdown": "# Plan"}),
            ["invalid_input", "invalid_input", REFUSED, HIDDEN, "invalid_input"],
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
                trashed_id: {
                    let id = entry("create_page", "Old");
                    catalogue.call(&core, &grace, "move_to_trash", json!({"id": id})).unwrap();
                    id
                },
                area_id: area_id.clone(),
                ken_id: match &ken {
                    Principal::Member { id, .. } => id.clone(),
                    _ => unreachable!(),
                },
                agent_id: "agent-archivist".into(),
                attachment_id: String::new(),
            };
            core.db()
                .execute("INSERT INTO agents (id, name, created_at) VALUES ('agent-archivist', 'Archivist', 0)", [])
                .unwrap();
            let attached = catalogue
                .call(&core, &grace, "attach_file", json!({"page_id": fixture.page_id, "name": "b.txt", "content": ""}))
                .unwrap()
                .output;
            let fixture = Fixture { attachment_id: attached["id"].as_str().unwrap().to_string(), ..fixture };
            let page = core.live().open(&fixture.page_id).unwrap();
            core.live().connected(&page);
            let Principal::Member { id: grace_id, .. } = &grace else { unreachable!() };
            core.live().write(&page, &crate::markdown::from_markdown("Notes for the team."), &crate::live::Author::member(grace_id));
            core.live().disconnected(&page).unwrap();
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

/// What a Personal Area's own Member cannot do there, though they are its Admin.
const NEVER_IN_A_PERSONAL_AREA: [&str; 5] = ["rename_area", "list_grants", "set_grant", "remove_grant", "delete_area"];

#[test]
fn a_personal_area_answers_to_its_member_alone() {
    let catalogue = catalogue();
    for (name, input, by_role) in table() {
        for caller_is in ["its Member", "another Member", "Workspace Admin"] {
            let core = fresh();
            let grace = member(&core, "Grace", false);
            let ken = member(&core, "Ken", false);
            let area_id = personal_area(&core, &grace);
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
                trashed_id: {
                    let id = entry("create_page", "Old");
                    catalogue.call(&core, &grace, "move_to_trash", json!({"id": id})).unwrap();
                    id
                },
                area_id: area_id.clone(),
                ken_id: match &ken {
                    Principal::Member { id, .. } => id.clone(),
                    _ => unreachable!(),
                },
                agent_id: "agent-archivist".into(),
                attachment_id: String::new(),
            };
            core.db()
                .execute("INSERT INTO agents (id, name, created_at) VALUES ('agent-archivist', 'Archivist', 0)", [])
                .unwrap();
            let attached = catalogue
                .call(&core, &grace, "attach_file", json!({"page_id": fixture.page_id, "name": "b.txt", "content": ""}))
                .unwrap()
                .output;
            let fixture = Fixture { attachment_id: attached["id"].as_str().unwrap().to_string(), ..fixture };
            core.db()
                .execute("INSERT INTO versions (page_id, number, markdown, recorded_at) VALUES (?1, 1, 'Notes.', 0)", [&fixture.page_id])
                .unwrap();
            let caller = match caller_is {
                "its Member" => grace.clone(),
                "another Member" => member(&core, "Linus", false),
                _ => member(&core, "Ada", true),
            };

            let input = input(&fixture);
            let text = input.to_string();
            let addresses_it = [&fixture.area_id, &fixture.page_id, &fixture.folder_id, &fixture.trashed_id, &fixture.attachment_id].iter().any(|id| text.contains(id.as_str()));
            let expected = match (caller_is, addresses_it) {
                ("its Member", true) if NEVER_IN_A_PERSONAL_AREA.contains(&name) => REFUSED,
                ("its Member", true) => by_role[0],
                ("its Member" | "another Member", false) => by_role[3],
                ("Workspace Admin", false) => by_role[4],
                (_, true) => HIDDEN,
                _ => unreachable!(),
            };
            let outcome = match catalogue.call(&core, &caller, name, input) {
                Ok(_) => OK,
                Err(error) => error.kind(),
            };
            assert_eq!(outcome, expected, "{name} called by {caller_is}");
        }
    }
}
