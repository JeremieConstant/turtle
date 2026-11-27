//! Importing over HTTP, against the real server: a ZIP to import may be far
//! larger than any other request, and only an import may be.
#![cfg(unix)]

mod support;

use base64::Engine;
use serde_json::json;
use std::io::Write;
use support::TestServer;

const MB: usize = 1024 * 1024;

/// A ZIP holding one Page of `bytes` bytes, stored without compression so
/// the ZIP is as large as the Page, in base64.
fn zip_of_one_page(bytes: usize) -> String {
    base64::engine::general_purpose::STANDARD.encode(zip_file_of_one_page(bytes))
}

/// The same ZIP as the file it is.
fn zip_file_of_one_page(bytes: usize) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("Runbook.md", options).unwrap();
    zip.write_all("a".repeat(bytes).as_bytes()).unwrap();
    zip.finish().unwrap().into_inner()
}

/// How many files lie where the server keeps uploads.
fn uploads_kept(server: &TestServer) -> usize {
    std::fs::read_dir(server.data_dir().join("uploads")).map(Iterator::count).unwrap_or(0)
}

#[tokio::test]
async fn an_import_may_be_larger_than_other_requests_up_to_its_own_limit() {
    let mut server = TestServer::start_with_limits(1, 3).await;
    let ada = server.sign_up().await;
    let area = ada.call("create_area", json!({ "name": "Engineering" })).await["id"].as_str().unwrap().to_string();
    let page = ada.create_page("Notes").await;

    let imported = ada.call("import", json!({ "area_id": area, "content": zip_of_one_page(2 * MB) })).await;
    let (too_large, refused) = ada.try_call("import", json!({ "area_id": area, "content": zip_of_one_page(3 * MB + 100 * 1024) })).await;
    let (other, other_refused) = ada.try_call("replace_page", json!({ "page_id": page, "markdown": "a".repeat(2 * MB) })).await;

    // A Confluence import has the same limit: this ZIP is no export, which is
    // only found once it is past the limit.
    let (confluence, not_an_export) = ada.try_call("check_confluence_import", json!({ "content": zip_of_one_page(2 * MB) })).await;
    let (confluence_too_large, _) = ada.try_call("import_confluence", json!({ "area_id": area, "content": zip_of_one_page(3 * MB + 100 * 1024) })).await;

    assert_eq!(imported["entries"][0]["title"], "Runbook");
    assert_eq!(confluence, 400);
    assert!(not_an_export.contains("not a Confluence space export"), "{not_an_export}");
    assert_eq!(confluence_too_large, 413);
    assert_eq!(too_large, 413);
    assert!(refused.contains("This ZIP is larger than 3 MB"), "{refused}");
    assert_eq!(other, 413);
    assert!(other_refused.contains("This request is larger than"), "{other_refused}");
    server.stop().await;
}

#[tokio::test]
async fn a_started_import_answers_at_once_and_is_followed_until_it_is_done() {
    let mut server = TestServer::start_with_limits(1, 3).await;
    let ada = server.sign_up().await;
    let area = ada.call("create_area", json!({ "name": "Engineering" })).await["id"].as_str().unwrap().to_string();

    let started = ada.call("start_import", json!({ "area_id": area, "content": zip_of_one_page(2 * MB) })).await;
    let (too_large, _) = ada.try_call("start_import", json!({ "area_id": area, "content": zip_of_one_page(3 * MB + 100 * 1024) })).await;
    let import_id = started["import_id"].as_str().unwrap();
    let mut progress = started.clone();
    for _ in 0..500 {
        progress = ada.call("get_import_progress", json!({ "import_id": import_id })).await;
        if progress["stage"] == "done" || progress["stage"] == "failed" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    assert_eq!(started["stage"], "reading");
    assert_eq!(too_large, 413);
    assert_eq!(progress["stage"], "done", "{progress}");
    assert_eq!(progress["imported"]["entries"][0]["title"], "Runbook");
    server.stop().await;
}

#[tokio::test]
async fn a_zip_is_uploaded_as_a_file_ahead_of_the_import_that_reads_and_removes_it() {
    let mut server = TestServer::start_with_limits(1, 3).await;
    let ada = server.sign_up().await;
    let area = ada.call("create_area", json!({ "name": "Engineering" })).await["id"].as_str().unwrap().to_string();

    // Larger than any other request may be, and sent as it is, not in base64.
    let (status, uploaded) = ada.upload("application/zip", &zip_file_of_one_page(2 * MB)).await;
    assert_eq!(status, 200, "{uploaded}");
    let uploaded: serde_json::Value = serde_json::from_str(&uploaded).unwrap();
    assert_eq!(uploads_kept(&server), 1);
    let started = ada.call("start_import", json!({ "area_id": area, "upload_id": uploaded["upload_id"] })).await;
    let import_id = started["import_id"].as_str().unwrap();
    let mut progress = started.clone();
    for _ in 0..500 {
        progress = ada.call("get_import_progress", json!({ "import_id": import_id })).await;
        if progress["stage"] == "done" || progress["stage"] == "failed" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    assert!(uploaded["bytes"].as_u64().unwrap() > 2 * MB as u64, "{uploaded}");
    assert_eq!(progress["stage"], "done", "{progress}");
    assert_eq!(progress["imported"]["entries"][0]["title"], "Runbook");
    assert_eq!(uploads_kept(&server), 0);
    server.stop().await;
}

#[tokio::test]
async fn an_upload_is_refused_when_too_large_from_nobody_or_as_a_form_and_leaves_nothing() {
    let mut server = TestServer::start_with_limits(1, 3).await;
    let ada = server.sign_up().await;
    let zip = zip_file_of_one_page(1024);

    let (too_large, why) = ada.upload("application/zip", &zip_file_of_one_page(3 * MB + 100 * 1024)).await;
    // What a form on another site could send with Ada's cookie.
    let (as_form, _) = ada.upload("application/x-www-form-urlencoded", &zip).await;
    let (as_text, how) = ada.upload("text/plain", &zip).await;
    let nobody = support::http::request_with_bytes(server.addr(), "POST", "/api/uploads", &[("Content-Type", "application/zip")], &zip).await;

    assert_eq!(too_large, 400);
    assert!(why.contains("This ZIP is larger than 3 MB, the most this Turtle imports at once."), "{why}");
    assert_eq!((as_form, as_text), (400, 400));
    assert!(how.contains("Send the ZIP as the body of the request, as application/zip."), "{how}");
    assert_eq!(nobody.status, 403, "{}", nobody.body);
    assert_eq!(uploads_kept(&server), 0);
    server.stop().await;
}
