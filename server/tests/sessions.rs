//! Open editors as the end of a session reaches them, tested from the outside
//! against the real server: whichever way a session ends, the editors opened
//! with it close at once, without sending anything and well before the
//! periodic access check, and nobody else's editor notices.
#![cfg(unix)]

mod support;

use serde_json::json;
use support::{http, Member, TestClient, TestServer};

/// An Area of `admin` with one Page, and `writer` given Write on it.
/// Returns the Page's ID.
async fn page_shared_with(admin: &Member, writer: &Member) -> String {
    let area = admin.call("create_area", json!({ "name": "Engineering" })).await;
    let page = admin.call("create_page", json!({ "area_id": area["id"], "title": "Roadmap" })).await;
    let holder = json!({ "kind": "member", "member_id": writer.id().await });
    admin.call("set_grant", json!({ "area_id": area["id"], "holder": holder, "role": "write" })).await;
    page["id"].as_str().unwrap().into()
}

/// Shows that `editor` still sends and receives changes: what it writes
/// reaches an editor opened afterwards by `member`, and what that one writes
/// reaches it.
async fn still_works(server: &TestServer, member: &Member, page: &str, editor: &TestClient, text: &str) {
    editor.append_paragraph(text);
    let late = server.connect(member, page).await;
    late.wait_until("the editor's change lands", |content| content.paragraphs.iter().any(|p| p == text))
        .await;
    late.append_paragraph("Answer");
    editor
        .wait_until("the editor receives a change", |content| content.paragraphs.iter().any(|p| p == "Answer"))
        .await;
}

#[tokio::test]
async fn deactivating_a_member_closes_all_of_their_open_editors_at_once() {
    let mut server = TestServer::start().await;
    let ada = server.sign_up().await;
    let ken = server.invite(&ada, "Ken").await;
    let page = page_shared_with(&ada, &ken).await;
    let first = server.connect(&ken, &page).await;
    let second = server.connect(&ken, &page).await;
    let unaffected = server.connect(&ada, &page).await;

    ada.call("deactivate_member", json!({ "id": ken.id().await })).await;

    first.wait_until_closed("the server closes Ken's first editor").await;
    second.wait_until_closed("the server closes Ken's second editor").await;
    still_works(&server, &ada, &page, &unaffected, "Ada keeps editing").await;
    server.stop().await;
}

#[tokio::test]
async fn signing_out_closes_the_editors_of_that_session_and_no_others() {
    let mut server = TestServer::start().await;
    let leaving = server.sign_up().await;
    let page = leaving.create_page("Roadmap").await;
    let staying = server.sign_in().await;
    let signed_out = server.connect(&leaving, &page).await;
    let other_session = server.connect(&staying, &page).await;

    leaving.call("sign_out", json!({})).await;
    // Sent at once, so it races the session's end to the signed-out editor.
    other_session.append_paragraph("After signing out");

    signed_out.wait_until_closed("the server closes the signed-out editor").await;
    assert!(signed_out.paragraphs().is_empty(), "{:?}", signed_out.paragraphs());
    still_works(&server, &staying, &page, &other_session, "Still editing").await;
    server.stop().await;
}

#[tokio::test]
async fn setting_a_new_password_closes_the_editors_of_a_members_other_sessions() {
    let mut server = TestServer::start().await;
    let ada = server.sign_up().await;
    let ken = server.invite(&ada, "Ken").await;
    let page = page_shared_with(&ada, &ken).await;
    let kens_editor = server.connect(&ken, &page).await;
    let unaffected = server.connect(&ada, &page).await;

    let link = ada.call("create_password_reset", json!({ "id": ken.id().await })).await;
    let secret = link["address"].as_str().unwrap().rsplit('/').next().unwrap().to_string();
    let reset = http::request(
        server.addr(),
        "POST",
        "/api/reset_password",
        &[("Content-Type", "application/json")],
        &json!({ "secret": secret, "password": "a brand new password" }).to_string(),
    )
    .await;
    assert_eq!(reset.status, 200, "resetting the password: {}", reset.body);

    kens_editor.wait_until_closed("the server closes the editor of Ken's old session").await;
    let new_session = reset.header("set-cookie").and_then(|value| value.split(';').next()).unwrap();
    assert_eq!(TestClient::try_connect(server.addr(), &page, Some(new_session)).await, 101);
    still_works(&server, &ada, &page, &unaffected, "Ada keeps editing").await;
    server.stop().await;
}
