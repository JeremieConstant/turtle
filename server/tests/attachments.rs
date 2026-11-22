//! Attachments over HTTP, against the real server: readers download them,
//! others cannot, and the largest file is whatever Turtle was started with.
#![cfg(unix)]

mod support;

use serde_json::json;
use support::{http, TestServer};

/// `bytes` bytes of the letter a, in base64.
fn base64_of_repeated_a(bytes: usize) -> String {
    // Three "a" are "YWFh"; the rest is padded.
    let mut encoded = "YWFh".repeat(bytes / 3);
    encoded.push_str(match bytes % 3 {
        0 => "",
        1 => "YQ==",
        _ => "YWE=",
    });
    encoded
}

#[tokio::test]
async fn readers_download_an_attachment_and_nobody_else_does() {
    let mut server = TestServer::start().await;
    let ada = server.sign_up().await;
    let page = ada.create_page("Runbook").await;
    // "hello" in base64.
    let attached = ada
        .call("attach_file", json!({ "page_id": page, "name": "deploy notes.txt", "content": "aGVsbG8=" }))
        .await;
    let url = attached["url"].as_str().unwrap();

    let download = http::request(server.addr(), "GET", url, &[("Cookie", &ada.cookie)], "").await;
    assert_eq!(download.status, 200);
    assert_eq!(download.body, "hello");
    assert_eq!(download.header("Content-Type"), Some("text/plain"));
    assert_eq!(
        download.header("Content-Disposition"),
        Some("attachment; filename*=UTF-8''deploy%20notes.txt")
    );
    assert_eq!(download.header("X-Content-Type-Options"), Some("nosniff"));

    let anonymous = http::request(server.addr(), "GET", url, &[], "").await;
    assert_eq!(anonymous.status, 401);
    let grace = server.invite(&ada, "Grace").await;
    let without_read = http::request(server.addr(), "GET", url, &[("Cookie", &grace.cookie)], "").await;
    let made_up = http::request(server.addr(), "GET", "/api/attachments/made-up", &[("Cookie", &grace.cookie)], "").await;
    assert_eq!(without_read.status, 404);
    assert_eq!(without_read.body, made_up.body);
    server.stop().await;
}

#[tokio::test]
async fn an_image_is_shown_rather_than_downloaded() {
    let mut server = TestServer::start().await;
    let ada = server.sign_up().await;
    let page = ada.create_page("Runbook").await;
    let attached = ada
        .call("attach_file", json!({ "page_id": page, "name": "diagram.png", "content": "aGVsbG8=" }))
        .await;
    let url = attached["url"].as_str().unwrap();
    let shown = http::request(server.addr(), "GET", url, &[("Cookie", &ada.cookie)], "").await;
    assert_eq!(shown.header("Content-Type"), Some("image/png"));
    assert_eq!(shown.header("Content-Disposition"), Some("inline; filename*=UTF-8''diagram.png"));
    server.stop().await;
}

#[tokio::test]
async fn files_up_to_the_default_limit_of_25_mb_are_taken() {
    let mut server = TestServer::start().await;
    let ada = server.sign_up().await;
    let page = ada.create_page("Runbook").await;
    // Well beyond what a web framework takes in a request by default.
    let content = base64_of_repeated_a(5 * 1024 * 1024);
    let attached = ada.call("attach_file", json!({ "page_id": page, "name": "big.bin", "content": content })).await;
    assert_eq!(attached["size"], 5 * 1024 * 1024);
    server.stop().await;
}

#[tokio::test]
async fn the_attachment_limit_is_set_when_turtle_starts() {
    let mut server = TestServer::start_with_attachment_limit(1).await;
    let ada = server.sign_up().await;
    let page = ada.create_page("Runbook").await;
    let mb = 1024 * 1024;

    let (status, body) = ada
        .try_call("attach_file", json!({ "page_id": page, "name": "big.bin", "content": base64_of_repeated_a(mb + 1) }))
        .await;
    assert_eq!(status, 400);
    assert!(body.contains("This file is larger than 1 MB"), "{body}");
    ada.call("attach_file", json!({ "page_id": page, "name": "fits.bin", "content": base64_of_repeated_a(mb) })).await;
    server.stop().await;
}
