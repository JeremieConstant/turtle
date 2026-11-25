//! A Demo as everyone and the operator see it: what `get_session` tells
//! everyone, and the settings Turtle refuses to start with.

mod support;

use serde_json::{json, Value};
use support::{http, DemoEnv, TestServer};

fn demo(email: &str, reset_hours: Option<&str>) -> DemoEnv {
    DemoEnv {
        email: Some(email.into()),
        password: Some("try turtle now".into()),
        reset_hours: reset_hours.map(Into::into),
    }
}

/// `get_session` as someone who is not signed in.
async fn session_signed_out(server: &TestServer) -> Value {
    let response = http::request(
        server.addr(),
        "POST",
        "/api/get_session",
        &[("Content-Type", "application/json")],
        "{}",
    )
    .await;
    assert_eq!(response.status, 200, "{}", response.body);
    serde_json::from_str(&response.body).unwrap()
}

#[tokio::test]
async fn without_demo_settings_the_session_has_no_demo() {
    let server = TestServer::start().await;
    let session = session_signed_out(&server).await;
    assert!(session.get("demo").is_none(), "{session}");
}

#[tokio::test]
async fn a_demo_tells_everyone_its_interval_and_the_account_of_a_member() {
    let server = TestServer::start_as_demo(demo("grace@example.org", Some("6"))).await;
    let ada = server.sign_up().await;
    let grace = server.invite(&ada, "Grace").await;

    let expected = json!({
        "resets_every_hours": 6,
        "account": { "email": "grace@example.org", "password": "try turtle now" },
    });
    assert_eq!(session_signed_out(&server).await["demo"], expected);
    assert_eq!(grace.call("get_session", json!({})).await["demo"], expected);
    assert_eq!(ada.call("get_session", json!({})).await["demo"], expected);
}

#[tokio::test]
async fn a_demo_without_an_interval_says_null() {
    let server = TestServer::start_as_demo(demo("grace@example.org", None)).await;
    let ada = server.sign_up().await;
    server.invite(&ada, "Grace").await;

    let demo = &session_signed_out(&server).await["demo"];
    assert_eq!(demo["resets_every_hours"], Value::Null);
    assert_eq!(demo["account"]["email"], "grace@example.org");
}

#[tokio::test]
async fn a_demo_shows_no_account_before_a_member_has_its_email_address() {
    let server = TestServer::start_as_demo(demo("grace@example.org", Some("6"))).await;
    let demo = &session_signed_out(&server).await["demo"];
    assert_eq!(demo, &json!({ "resets_every_hours": 6, "account": null }));
}

#[tokio::test]
async fn a_demo_never_publishes_a_workspace_admins_password_and_says_why_once() {
    // `sign_up` creates the Workspace Admin ada@example.org.
    let server = TestServer::start_as_demo(demo("ada@example.org", Some("6"))).await;
    server.sign_up().await;

    for _ in 0..2 {
        let demo = &session_signed_out(&server).await["demo"];
        assert_eq!(demo, &json!({ "resets_every_hours": 6, "account": null }));
    }
    let warnings = server
        .logged()
        .await
        .into_iter()
        .filter(|line| line.contains("Workspace Admin's password"))
        .count();
    assert_eq!(warnings, 1);
}

#[tokio::test]
async fn turtle_refuses_to_start_with_demo_settings_that_do_not_fit() {
    let password_alone = DemoEnv { email: None, ..demo("grace@example.org", None) };
    let email_alone = DemoEnv { password: None, ..demo("grace@example.org", None) };
    let spaces_for_email = DemoEnv { email: Some("  ".into()), ..demo("grace@example.org", None) };
    let spaces_for_password = DemoEnv { password: Some("  ".into()), ..demo("grace@example.org", None) };
    let interval_alone = DemoEnv { reset_hours: Some("6".into()), ..DemoEnv::default() };
    let cases = [
        (password_alone, "TURTLE_DEMO_EMAIL"),
        (email_alone, "TURTLE_DEMO_PASSWORD"),
        (spaces_for_email, "TURTLE_DEMO_EMAIL"),
        (spaces_for_password, "TURTLE_DEMO_PASSWORD"),
        (interval_alone, "TURTLE_DEMO_RESET_HOURS"),
        (demo("grace@example.org", Some("0")), "TURTLE_DEMO_RESET_HOURS"),
        (demo("grace@example.org", Some("1.5")), "TURTLE_DEMO_RESET_HOURS"),
        (demo("grace@example.org", Some("six")), "TURTLE_DEMO_RESET_HOURS"),
    ];
    for (settings, named) in cases {
        let described = format!("{settings:?}");
        let said = TestServer::refuses_to_start_as_demo(settings).await;
        assert!(said.contains(&format!("{named} must")), "{described}: {said}");
    }
}
