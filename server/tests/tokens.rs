//! Tokens from the outside, against the real server: an Agent reads and
//! writes through the HTTP API and through the MCP endpoint with a Token, and
//! the Token's limits hold in both.
#![cfg(unix)]

mod support;

use serde_json::{json, Value};
use support::{http, Member, TestServer};

/// Creates a Token for `owner` and returns its value.
async fn token(owner: &Member, input: Value) -> String {
    let created = owner.call("create_token", input).await;
    created["value"].as_str().expect("a new Token shows its value").to_string()
}

/// Calls an operation of the HTTP API with a Token.
async fn api(server: &TestServer, token: &str, operation: &str, input: Value) -> http::Response {
    let bearer = format!("Bearer {token}");
    http::request(
        server.addr(),
        "POST",
        &format!("/api/{operation}"),
        &[("Content-Type", "application/json"), ("Authorization", &bearer)],
        &input.to_string(),
    )
    .await
}

/// Sends one JSON-RPC message to the MCP endpoint, as an MCP client does.
async fn mcp(server: &TestServer, token: Option<&str>, message: Value) -> http::Response {
    let bearer = token.map(|token| format!("Bearer {token}"));
    let mut headers = vec![
        ("Content-Type", "application/json"),
        ("Accept", "application/json, text/event-stream"),
        ("MCP-Protocol-Version", "2025-06-18"),
    ];
    if let Some(bearer) = &bearer {
        headers.push(("Authorization", bearer));
    }
    http::request(server.addr(), "POST", "/mcp", &headers, &message.to_string()).await
}

/// Calls an MCP tool and returns the JSON-RPC result.
async fn call_tool(server: &TestServer, token: &str, name: &str, arguments: Value) -> Value {
    let message = json!({
        "jsonrpc": "2.0", "id": 7, "method": "tools/call",
        "params": { "name": name, "arguments": arguments },
    });
    let response = mcp(server, Some(token), message).await;
    assert_eq!(response.status, 200, "{name}: {}", response.body);
    let answer: Value = serde_json::from_str(&response.body).expect("MCP answers with JSON");
    assert_eq!(answer["id"], 7, "{answer}");
    answer["result"].clone()
}

#[tokio::test]
async fn an_agent_reads_and_writes_through_the_mcp_endpoint_with_a_token() {
    let server = TestServer::start().await;
    let ada = server.sign_up().await;
    let token = token(&ada, json!({ "name": "Claude" })).await;

    let initialize = json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "1" },
        },
    });
    let response = mcp(&server, Some(&token), initialize).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let initialized: Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(initialized["result"]["protocolVersion"], "2025-06-18");
    assert!(initialized["result"]["capabilities"]["tools"].is_object(), "{initialized}");
    let notified = mcp(&server, Some(&token), json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;
    assert_eq!(notified.status, 202);
    for invalid in [json!([]), json!(5), json!({ "jsonrpc": "2.0", "id": 9 })] {
        let response = mcp(&server, Some(&token), invalid.clone()).await;
        let answer: Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(answer["error"]["code"], -32600, "{invalid}: {answer}");
    }

    let listed = mcp(&server, Some(&token), json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).await;
    let listed: Value = serde_json::from_str(&listed.body).unwrap();
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"create_page") && names.contains(&"get_tree"), "{names:?}");
    assert!(!names.contains(&"create_token"), "{names:?}");

    // A write, then a read that sees it.
    let area = call_tool(&server, &token, "create_area", json!({ "name": "Engineering" })).await;
    assert_eq!(area["isError"], false, "{area}");
    let area_id = area["structuredContent"]["id"].as_str().unwrap().to_string();
    let page = call_tool(&server, &token, "create_page", json!({ "area_id": area_id, "title": "Roadmap" })).await;
    assert_eq!(page["isError"], false, "{page}");
    let tree = call_tool(&server, &token, "get_tree", json!({ "area_id": area_id })).await;
    assert_eq!(tree["structuredContent"]["entries"][0]["title"], "Roadmap", "{tree}");
    let text = tree["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Roadmap"), "{text}");

    // What the Agent created belongs to the Token's owner.
    let areas = ada.call("list_areas", json!({})).await;
    assert_eq!(areas["areas"][0]["name"], "Engineering");
}

#[tokio::test]
async fn the_mcp_endpoint_needs_a_valid_token() {
    let server = TestServer::start().await;
    let ada = server.sign_up().await;
    let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });

    let without = mcp(&server, None, list.clone()).await;
    assert_eq!(without.status, 401);
    assert!(without.header("www-authenticate").is_some_and(|value| value.starts_with("Bearer")));
    let made_up = mcp(&server, Some("turtle_0123"), list.clone()).await;
    assert_eq!(made_up.status, 401);

    // A browser session is not enough: the endpoint ignores cookies.
    let with_cookie = http::request(
        server.addr(),
        "POST",
        "/mcp",
        &[("Content-Type", "application/json"), ("Accept", "application/json, text/event-stream"), ("Cookie", &ada.cookie)],
        &list.to_string(),
    )
    .await;
    assert_eq!(with_cookie.status, 401);

    let revoked = token(&ada, json!({ "name": "Revoked" })).await;
    let tokens = ada.call("list_tokens", json!({})).await;
    ada.call("revoke_token", json!({ "id": tokens["tokens"][0]["id"] })).await;
    assert_eq!(mcp(&server, Some(&revoked), list).await.status, 401);
}

#[tokio::test]
async fn a_tokens_limits_hold_through_mcp_and_the_http_api() {
    let server = TestServer::start().await;
    let ada = server.sign_up().await;
    let engineering = ada.call("create_area", json!({ "name": "Engineering" })).await;
    let secret = ada.call("create_area", json!({ "name": "Secret" })).await;
    let page = ada.call("create_page", json!({ "area_id": secret["id"], "title": "Plans" })).await;
    let limited = token(&ada, json!({ "name": "Reader", "read_only": true, "area_ids": [engineering["id"]] })).await;

    let areas = call_tool(&server, &limited, "list_areas", json!({})).await;
    let names: Vec<_> = areas["structuredContent"]["areas"].as_array().unwrap().iter().map(|area| area["name"].clone()).collect();
    assert_eq!(names, [json!("Engineering")]);

    let hidden = call_tool(&server, &limited, "get_page", json!({ "id": page["id"] })).await;
    assert_eq!(hidden["isError"], true);
    assert_eq!(hidden["content"][0]["text"], "There is no such Page.");

    let write = call_tool(&server, &limited, "create_page", json!({ "area_id": engineering["id"], "title": "New" })).await;
    assert_eq!(write["isError"], true, "{write}");

    let through_api = api(&server, &limited, "list_areas", json!({})).await;
    assert_eq!(through_api.status, 200, "{}", through_api.body);
    let listed: Value = serde_json::from_str(&through_api.body).unwrap();
    assert_eq!(listed["areas"].as_array().unwrap().len(), 1);
    let refused = api(&server, &limited, "rename_area", json!({ "id": engineering["id"], "name": "Renamed" })).await;
    assert_eq!(refused.status, 403);
    let markdown = http::request(
        server.addr(),
        "GET",
        &format!("/api/pages/{}/markdown", page["id"].as_str().unwrap()),
        &[("Authorization", &format!("Bearer {limited}"))],
        "",
    )
    .await;
    assert_eq!(markdown.status, 404);
    let unknown = api(&server, "turtle_0123", "list_areas", json!({})).await;
    assert_eq!(unknown.status, 401);
}

#[tokio::test]
async fn deactivating_a_member_disables_their_tokens() {
    let server = TestServer::start().await;
    let ada = server.sign_up().await;
    let ken = server.invite(&ada, "Ken").await;
    let kens_token = token(&ken, json!({ "name": "Agent" })).await;
    assert_eq!(api(&server, &kens_token, "list_areas", json!({})).await.status, 200);

    ada.call("deactivate_member", json!({ "id": ken.id().await })).await;

    assert_eq!(api(&server, &kens_token, "list_areas", json!({})).await.status, 401);
    let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
    assert_eq!(mcp(&server, Some(&kens_token), list).await.status, 401);
}
