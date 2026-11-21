//! Who may open a Page, tested from the outside against the real server: the
//! editing WebSocket and the Markdown of a Page only serve Pages of an Area
//! to someone signed in who can read them.
#![cfg(unix)]

mod support;

use serde_json::json;
use support::{http, TestClient, TestServer};

#[tokio::test]
async fn the_editing_websocket_needs_a_valid_session() {
    let mut server = TestServer::start().await;
    let member = server.sign_up().await;
    let page = member.create_page("Roadmap").await;

    let anonymous = TestClient::try_connect(server.addr(), &page, None).await;
    let forged = TestClient::try_connect(server.addr(), &page, Some("turtle_session=0123abcd")).await;
    let signed_in = TestClient::try_connect(server.addr(), &page, Some(&member.cookie)).await;

    assert_eq!(anonymous, 401);
    assert_eq!(forged, 401);
    assert_eq!(signed_in, 101);
    server.stop().await;
}

#[tokio::test]
async fn an_open_editor_stops_changing_the_page_once_its_session_ends() {
    let mut server = TestServer::start().await;
    let leaving = server.sign_up().await;
    let page = leaving.create_page("Roadmap").await;
    let staying = server.sign_in().await;
    let editor = server.connect(&leaving, &page).await;
    editor.append_paragraph("Before signing out");
    let observer = server.connect(&staying, &page).await;
    observer
        .wait_until("the observer sees the first edit", |content| {
            content.paragraphs == ["Before signing out"]
        })
        .await;

    leaving.call("sign_out", json!({})).await;
    editor.append_paragraph("After signing out");

    editor.wait_until_closed("the server closes the signed-out editor").await;
    // Opened afterwards, so it holds whatever the server applied.
    let late = server.connect(&staying, &page).await;
    assert_eq!(late.paragraphs(), ["Before signing out"]);
    server.stop().await;
}

#[tokio::test]
async fn only_pages_of_an_area_can_be_opened() {
    let mut server = TestServer::start().await;
    let member = server.sign_up().await;
    let area = member.call("create_area", json!({ "name": "Engineering" })).await;
    let folder = member
        .call("create_folder", json!({ "area_id": area["id"], "title": "Guides" }))
        .await;
    let folder = folder["id"].as_str().unwrap();
    let cookie = Some(member.cookie.as_str());

    // "roadmap" stands for a Page of the first slice, which had no Area.
    for missing in ["roadmap", folder] {
        let status = TestClient::try_connect(server.addr(), missing, cookie).await;
        assert_eq!(status, 404, "{missing}");
    }
    server.stop().await;
}

#[tokio::test]
async fn the_markdown_of_a_page_needs_a_session_and_a_page_of_an_area() {
    let mut server = TestServer::start().await;
    let member = server.sign_up().await;
    let page = member.create_page("Roadmap").await;
    let signed_in = [("Cookie", member.cookie.as_str())];
    let markdown = |page: &str| format!("/api/pages/{page}/markdown");

    let written = http::request(server.addr(), "PUT", &markdown(&page), &signed_in, "# Q1\n").await;
    let read = http::request(server.addr(), "GET", &markdown(&page), &signed_in, "").await;
    assert_eq!(written.status, 204);
    assert_eq!((read.status, read.body.as_str()), (200, "# Q1\n"));

    let anonymous_read = http::request(server.addr(), "GET", &markdown(&page), &[], "").await;
    let anonymous_write = http::request(server.addr(), "PUT", &markdown(&page), &[], "# No\n").await;
    let old_page = http::request(server.addr(), "GET", &markdown("roadmap"), &signed_in, "").await;
    assert_eq!(anonymous_read.status, 401);
    assert_eq!(anonymous_write.status, 401);
    assert_eq!(old_page.status, 404);

    let unchanged = http::request(server.addr(), "GET", &markdown(&page), &signed_in, "").await;
    assert_eq!(unchanged.body, "# Q1\n");
    server.stop().await;
}
