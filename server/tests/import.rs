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
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("Runbook.md", options).unwrap();
    zip.write_all("a".repeat(bytes).as_bytes()).unwrap();
    base64::engine::general_purpose::STANDARD.encode(zip.finish().unwrap().into_inner())
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
