//! Versions, tested from the outside: the real server, editing over
//! WebSocket and operations over HTTP. The timing rules are tested in the
//! core with a clock that only moves when told; here only the closing of the
//! last connection can be waited for.
#![cfg(unix)]

mod support;

use serde_json::{json, Value};
use std::time::Duration;
use support::{Member, TestServer};

/// The Versions of a Page once there are `count`, as the server records them
/// in the background after the last connection has closed.
async fn versions_once_there_are(member: &Member, page: &str, count: usize) -> Vec<Value> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let listed = member.call("list_versions", json!({ "page_id": page })).await;
        let versions = listed["versions"].as_array().unwrap().clone();
        if versions.len() >= count {
            return versions;
        }
        assert!(tokio::time::Instant::now() < deadline, "only {} Versions: {listed}", versions.len());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn a_version_naming_its_author_is_recorded_when_the_last_editor_leaves() {
    let mut server = TestServer::start().await;
    let ada = server.sign_up().await;
    let page = ada.create_page("Shared notes").await;
    let writer = server.connect(&ada, &page).await;
    let reader = server.connect(&ada, &page).await;
    writer.append_paragraph("Agenda for Monday.");
    reader.wait_until("the server has the change", |content| content.paragraphs.len() == 1).await;

    writer.disconnect().await;
    reader.disconnect().await;

    let versions = versions_once_there_are(&ada, &page, 1).await;
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0]["number"], 1);
    assert_eq!(versions[0]["authors"], json!([{ "id": ada.id().await, "name": "Ada" }]));
    let first = ada.call("get_version", json!({ "page_id": page, "number": 1 })).await;
    assert_eq!(first["markdown"], "Agenda for Monday.\n");
    server.stop().await;
}

#[tokio::test]
async fn a_restore_reaches_the_people_editing_at_that_moment() {
    let mut server = TestServer::start().await;
    let ada = server.sign_up().await;
    let page = ada.create_page("Shared notes").await;
    let first = server.connect(&ada, &page).await;
    first.append_paragraph("Agenda for Monday.");
    first.wait_until("the change is applied", |content| content.paragraphs.len() == 1).await;
    first.disconnect().await;
    versions_once_there_are(&ada, &page, 1).await;

    let editor = server.connect(&ada, &page).await;
    editor.append_paragraph("Bring snacks.");
    // Another connection sees the change only once the server has applied it.
    let observer = server.connect(&ada, &page).await;
    observer.wait_until("the server has the change", |content| content.paragraphs.len() == 2).await;

    let restored = ada.call("restore_version", json!({ "page_id": page, "number": 1 })).await;

    assert_eq!(restored["version"]["number"], 3);
    assert_eq!(restored["version"]["restored_from"], 1);
    editor
        .wait_until("the editor sees the restore", |content| content.paragraphs == ["Agenda for Monday."])
        .await;
    let second = ada.call("get_version", json!({ "page_id": page, "number": 2 })).await;
    assert_eq!(second["markdown"], "Agenda for Monday.\n\nBring snacks.\n");
    server.stop().await;
}
