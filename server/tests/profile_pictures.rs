//! A Profile picture over HTTP, against the real server: shown as an image to
//! whoever sees the Profile, and gone once it is removed.
#![cfg(unix)]

mod support;

use serde_json::json;
use support::{http, TestServer};

/// A PNG of one green pixel, in base64.
const PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

#[tokio::test]
async fn a_picture_is_shown_as_an_image_to_whoever_sees_the_profile() {
    let mut server = TestServer::start().await;
    let ada = server.sign_up().await;
    let grace = server.invite(&ada, "Grace").await;
    let profile = grace.call("set_profile_picture", json!({ "content": PNG_BASE64 })).await;
    let url = profile["picture"].as_str().unwrap();

    let shown = http::request(server.addr(), "GET", url, &[("Cookie", &ada.cookie)], "").await;
    assert_eq!(shown.status, 200);
    assert!(shown.body.contains("PNG"));
    assert_eq!(shown.header("Content-Type"), Some("image/png"));
    assert_eq!(shown.header("X-Content-Type-Options"), Some("nosniff"));
    assert_eq!(shown.header("Cache-Control"), Some("private"));

    let anonymous = http::request(server.addr(), "GET", url, &[], "").await;
    assert_eq!(anonymous.status, 401);
    ada.call("remove_profile_picture", json!({ "id": grace.id().await })).await;
    let removed = http::request(server.addr(), "GET", url, &[("Cookie", &grace.cookie)], "").await;
    assert_eq!(removed.status, 404);
    server.stop().await;
}
