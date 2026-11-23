//! A PDF Style's logo over HTTP, against the real server: Workspace Admins
//! see it as an image, nobody else does, and an SVG never runs as a page.
#![cfg(unix)]

mod support;

use serde_json::json;
use support::{http, TestServer};

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="#2f5f8a"/></svg>"##;

/// `SVG` in base64.
const SVG_BASE64: &str = "PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyMCIgaGVpZ2h0PSIxMCI+PHJlY3Qgd2lkdGg9IjIwIiBoZWlnaHQ9IjEwIiBmaWxsPSIjMmY1ZjhhIi8+PC9zdmc+";

#[tokio::test]
async fn workspace_admins_see_a_logo_as_an_image_and_nobody_else_does() {
    let mut server = TestServer::start().await;
    let ada = server.sign_up().await;
    let style = ada
        .call("set_pdf_style_logo", json!({ "pdf_style_id": "standard", "file_name": "mark.svg", "content": SVG_BASE64 }))
        .await;
    let url = style["logo"]["url"].as_str().unwrap();

    let shown = http::request(server.addr(), "GET", url, &[("Cookie", &ada.cookie)], "").await;
    assert_eq!(shown.status, 200);
    assert_eq!(shown.body, SVG);
    assert_eq!(shown.header("Content-Type"), Some("image/svg+xml"));
    // Opened on its own, it is downloaded, and could not run if it were shown.
    assert_eq!(shown.header("Content-Disposition"), Some("attachment; filename*=UTF-8''mark.svg"));
    assert_eq!(shown.header("Content-Security-Policy"), Some("sandbox"));
    assert_eq!(shown.header("X-Content-Type-Options"), Some("nosniff"));

    let anonymous = http::request(server.addr(), "GET", url, &[], "").await;
    assert_eq!(anonymous.status, 401);
    let grace = server.invite(&ada, "Grace").await;
    let not_admin = http::request(server.addr(), "GET", url, &[("Cookie", &grace.cookie)], "").await;
    assert_eq!(not_admin.status, 403);
    ada.call("remove_pdf_style_logo", json!({ "pdf_style_id": "standard" })).await;
    let removed = http::request(server.addr(), "GET", url, &[("Cookie", &ada.cookie)], "").await;
    assert_eq!(removed.status, 404);
    server.stop().await;
}
