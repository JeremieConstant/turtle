//! Failed attempts as the real server counts them: by the address a request
//! comes from, which behind a reverse proxy is taken from the header the
//! operator names, and otherwise from the connection whatever headers say.
#![cfg(unix)]

mod support;

use serde_json::json;
use support::{http, TestServer};

/// Tries to accept an invitation through a made-up link, saying it comes
/// through a proxy with `forwarded_for` in `X-Forwarded-For`. Returns the
/// message the server refuses it with.
async fn made_up_link(server: &TestServer, number: usize, forwarded_for: &str) -> String {
    let body = json!({ "secret": format!("{number:064x}"), "name": "Eve", "password": "a long enough password" });
    let response = http::request(
        server.addr(),
        "POST",
        "/api/accept_invitation",
        &[("Content-Type", "application/json"), ("X-Forwarded-For", forwarded_for)],
        &body.to_string(),
    )
    .await;
    assert_eq!(response.status, 403, "{}", response.body);
    let body: serde_json::Value = serde_json::from_str(&response.body).unwrap();
    body["error"]["message"].as_str().unwrap().to_string()
}

const UNUSABLE: &str = "This link has already been used or has expired. Ask a Workspace Admin for a new one.";
const TOO_MANY: &str = "Too many attempts failed. Try again in 15 minutes.";

#[tokio::test]
async fn behind_a_proxy_each_client_is_counted_by_the_address_the_proxy_adds_last() {
    let mut server = TestServer::start_with_client_address_header("X-Forwarded-For").await;
    // What the client itself put first changes nothing, nor a port the
    // proxy adds.
    for number in 0..20 {
        let port = if number % 2 == 0 { "" } else { ":40000" };
        let forwarded_for = format!("192.0.2.{number}, 203.0.113.5{port}");
        assert_eq!(made_up_link(&server, number, &forwarded_for).await, UNUSABLE);
    }

    assert_eq!(made_up_link(&server, 20, "192.0.2.99, 203.0.113.5").await, TOO_MANY);
    assert_eq!(made_up_link(&server, 21, "203.0.113.6").await, UNUSABLE);
    server.stop().await;
}

#[tokio::test]
async fn without_a_proxy_a_client_cannot_escape_counting_by_sending_the_header() {
    let mut server = TestServer::start().await;
    for number in 0..20 {
        let forwarded_for = format!("203.0.113.{number}");
        assert_eq!(made_up_link(&server, number, &forwarded_for).await, UNUSABLE);
    }

    assert_eq!(made_up_link(&server, 20, "203.0.113.200").await, TOO_MANY);
    server.stop().await;
}
