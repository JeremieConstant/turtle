//! Search against the real server: what is written becomes findable within a
//! few seconds, and a missing index is rebuilt on start.
#![cfg(unix)]

mod support;

use serde_json::json;
use std::time::{Duration, Instant};
use support::{http, Member, TestServer};

/// What "within a few seconds" means here.
const FINDABLE_WITHIN: Duration = Duration::from_secs(5);

/// Searches until `query` finds the Page, and says how long that took.
async fn wait_until_found(member: &Member, query: &str, page: &str) -> Duration {
    let started = Instant::now();
    loop {
        let found = member.call("search", json!({ "query": query })).await;
        let ids: Vec<&str> = found["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|result| result["page_id"].as_str().unwrap())
            .collect();
        if ids == [page] {
            return started.elapsed();
        }
        assert!(started.elapsed() < FINDABLE_WITHIN, "{query} did not find {page} in time: {found}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn an_edit_in_the_editor_is_findable_within_a_few_seconds() {
    let mut server = TestServer::start().await;
    let member = server.sign_up().await;
    let page = member.create_page("Roadmap").await;
    wait_until_found(&member, "roadmap", &page).await;
    let editor = server.connect(&member, &page).await;

    editor.append_paragraph("The Kubernetes rollout starts in March.");

    wait_until_found(&member, "kubernetes", &page).await;
    server.stop().await;
}

#[tokio::test]
async fn a_missing_index_is_rebuilt_on_start() {
    let mut server = TestServer::start().await;
    let member = server.sign_up().await;
    let page = member.create_page("Roadmap").await;
    let signed_in = [("Cookie", member.cookie.as_str())];
    let markdown = format!("/api/pages/{page}/markdown");
    let written = http::request(server.addr(), "PUT", &markdown, &signed_in, "Hire two engineers.\n").await;
    assert_eq!(written.status, 204);
    wait_until_found(&member, "engineers", &page).await;
    server.stop().await;

    std::fs::remove_dir_all(server.data_dir().join("search")).unwrap();
    server.restart().await;

    let member = server.sign_in().await;
    wait_until_found(&member, "engineers", &page).await;
    server.stop().await;
}
