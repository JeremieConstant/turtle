//! Grants as open editors feel them, tested from the outside against the real
//! server: someone with Read cannot change a Page, and a lowered or removed
//! Grant reaches an editor that is already open, without it reconnecting.
#![cfg(unix)]

mod support;

use serde_json::{json, Value};
use support::{http, Member, TestClient, TestServer};

/// An Area of `admin` with one Page, and `holder` given `role` on it.
/// Returns the Area's and the Page's ID.
async fn page_shared_with(admin: &Member, holder: &Member, role: &str) -> (String, String) {
    let area = admin.call("create_area", json!({ "name": "Engineering" })).await;
    let page = admin.call("create_page", json!({ "area_id": area["id"], "title": "Roadmap" })).await;
    let area_id = area["id"].as_str().unwrap().to_string();
    set_role(admin, &area_id, holder, role).await;
    (area_id, page["id"].as_str().unwrap().into())
}

/// A Member as the holder of a Grant.
async fn holder(member: &Member) -> Value {
    json!({ "kind": "member", "member_id": member.id().await })
}

async fn set_role(admin: &Member, area_id: &str, holder_member: &Member, role: &str) {
    let holder = holder(holder_member).await;
    admin.call("set_grant", json!({ "area_id": area_id, "holder": holder, "role": role })).await;
}

#[tokio::test]
async fn someone_with_read_cannot_change_a_page() {
    let mut server = TestServer::start().await;
    let ada = server.sign_up().await;
    let ken = server.invite(&ada, "Ken").await;
    let (_, page) = page_shared_with(&ada, &ken, "read").await;
    let reader = server.connect(&ken, &page).await;
    let observer = server.connect(&ada, &page).await;

    reader.append_paragraph("Written by a reader");
    // Frames of one connection are handled in order, so once the reader's
    // presence has arrived, its change has been dealt with too.
    reader.set_presence(r#"{"user":{"name":"Ken"}}"#);
    let reader_id = reader.client_id();
    observer
        .wait_until("the observer sees the reader's presence", |content| {
            content.others.contains_key(&reader_id)
        })
        .await;

    let markdown = http::request(
        server.addr(),
        "PUT",
        &format!("/api/pages/{page}/markdown"),
        &[("Cookie", &ken.cookie)],
        "Also written by a reader",
    )
    .await;
    assert_eq!(markdown.status, 403);
    let late = server.connect(&ada, &page).await;
    assert!(late.paragraphs().is_empty(), "{:?}", late.paragraphs());
    server.stop().await;
}

#[tokio::test]
async fn lowering_a_grant_closes_the_open_editor_at_once_and_it_reopens_with_read() {
    let mut server = TestServer::start().await;
    let ada = server.sign_up().await;
    let ken = server.invite(&ada, "Ken").await;
    let (area, page) = page_shared_with(&ada, &ken, "write").await;
    let editor = server.connect(&ken, &page).await;

    set_role(&ada, &area, &ken, "read").await;

    // Well before the periodic check, and without Ken sending anything.
    editor.wait_until_closed("the server closes the editor of the lowered Grant").await;
    let opened = ken.call("get_page", json!({ "id": page })).await;
    assert_eq!(opened["role"], "read");
    assert_eq!(TestClient::try_connect(server.addr(), &page, Some(&ken.cookie)).await, 101);
    server.stop().await;
}

#[tokio::test]
async fn removing_a_grant_closes_the_open_editor_and_hides_the_page() {
    let mut server = TestServer::start().await;
    let ada = server.sign_up().await;
    let ken = server.invite(&ada, "Ken").await;
    let (area, page) = page_shared_with(&ada, &ken, "write").await;
    let editor = server.connect(&ken, &page).await;
    let unaffected = server.connect(&ada, &page).await;

    ada.call("remove_grant", json!({ "area_id": area, "holder": holder(&ken).await })).await;

    editor.wait_until_closed("the server closes the editor of the removed Grant").await;
    assert_eq!(TestClient::try_connect(server.addr(), &page, Some(&ken.cookie)).await, 404);
    let (status, _) = ken.try_call("get_area", json!({ "id": area })).await;
    assert_eq!(status, 404);
    unaffected.append_paragraph("Still here");
    let late = server.connect(&ada, &page).await;
    late.wait_until("Ada's editor still works", |content| content.paragraphs == ["Still here"])
        .await;
    server.stop().await;
}
