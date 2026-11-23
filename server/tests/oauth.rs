//! Connecting an AI client through OAuth (ADR-0010), against the real
//! server: the client finds Turtle's metadata, registers itself, a Member
//! allows it, and the client then acts as that Member at the MCP endpoint.
#![cfg(unix)]

mod support;

use serde_json::{json, Value};
use support::{http, TestServer};

const PUBLIC_URL: &str = "https://turtle.example.org";

async fn get(server: &TestServer, path: &str) -> http::Response {
    http::request(server.addr(), "GET", path, &[], "").await
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

fn json_of(response: &http::Response) -> Value {
    serde_json::from_str(&response.body).unwrap_or_else(|_| panic!("not JSON: {}", response.body))
}

#[tokio::test]
async fn with_a_public_url_turtle_describes_itself_as_protected_resource_and_authorization_server() {
    let server = TestServer::start_with_public_url(PUBLIC_URL).await;

    for path in ["/.well-known/oauth-protected-resource/mcp", "/.well-known/oauth-protected-resource"] {
        let resource = get(&server, path).await;
        assert_eq!(resource.status, 200, "{path}: {}", resource.body);
        let resource = json_of(&resource);
        assert_eq!(resource["resource"], "https://turtle.example.org/mcp");
        assert_eq!(resource["authorization_servers"], json!(["https://turtle.example.org"]));
        assert_eq!(resource["scopes_supported"], json!(["turtle:read", "turtle:write"]));
    }

    let server_metadata = get(&server, "/.well-known/oauth-authorization-server").await;
    assert_eq!(server_metadata.status, 200, "{}", server_metadata.body);
    let metadata = json_of(&server_metadata);
    assert_eq!(metadata["issuer"], "https://turtle.example.org");
    assert_eq!(metadata["authorization_endpoint"], "https://turtle.example.org/oauth/authorize");
    assert_eq!(metadata["token_endpoint"], "https://turtle.example.org/oauth/token");
    assert_eq!(metadata["registration_endpoint"], "https://turtle.example.org/oauth/register");
    assert_eq!(metadata["response_types_supported"], json!(["code"]));
    assert_eq!(metadata["grant_types_supported"], json!(["authorization_code"]));
    assert_eq!(metadata["code_challenge_methods_supported"], json!(["S256"]));
    assert_eq!(metadata["token_endpoint_auth_methods_supported"], json!(["none"]));
    assert_eq!(metadata["scopes_supported"], json!(["turtle:read", "turtle:write"]));

    // A client that has no credentials learns where to find them.
    let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
    let refused = mcp(&server, None, list.clone()).await;
    assert_eq!(refused.status, 401);
    assert_eq!(
        refused.header("www-authenticate"),
        Some(r#"Bearer resource_metadata="https://turtle.example.org/.well-known/oauth-protected-resource/mcp""#)
    );
    let unknown = mcp(&server, Some("turtle_0123"), list).await;
    assert_eq!(unknown.status, 401);
    assert!(
        unknown.header("www-authenticate").is_some_and(|value| value.contains("resource_metadata=") && value.contains(r#"error="invalid_token""#)),
        "{:?}",
        unknown.header("www-authenticate")
    );
}

#[tokio::test]
async fn without_a_public_url_turtle_offers_no_oauth_and_tokens_keep_working() {
    let server = TestServer::start().await;
    let ada = server.sign_up().await;

    for path in [
        "/.well-known/oauth-protected-resource/mcp",
        "/.well-known/oauth-protected-resource",
        "/.well-known/oauth-authorization-server",
        "/oauth/authorize?client_id=x",
    ] {
        assert_eq!(get(&server, path).await.status, 404, "{path}");
    }
    let register = http::request(
        server.addr(),
        "POST",
        "/oauth/register",
        &[("Content-Type", "application/json")],
        &json!({ "redirect_uris": ["https://claude.ai/api/mcp/auth_callback"] }).to_string(),
    )
    .await;
    assert_eq!(register.status, 404);

    let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
    let refused = mcp(&server, None, list.clone()).await;
    assert_eq!(refused.status, 401);
    assert_eq!(refused.header("www-authenticate"), Some("Bearer"));

    let created = ada.call("create_token", json!({ "name": "Claude" })).await;
    let token = created["value"].as_str().unwrap();
    assert_eq!(mcp(&server, Some(token), list).await.status, 200);
}

const CALLBACK: &str = "https://claude.ai/api/mcp/auth_callback";
/// The worked example of RFC 7636, appendix B.
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

/// Registers a client the way Claude does, and returns its client ID.
async fn register(server: &TestServer, name: &str) -> String {
    let response = http::request(
        server.addr(),
        "POST",
        "/oauth/register",
        &[("Content-Type", "application/json")],
        &json!({
            "client_name": name,
            "redirect_uris": [CALLBACK],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        })
        .to_string(),
    )
    .await;
    assert_eq!(response.status, 201, "{}", response.body);
    let registered = json_of(&response);
    assert_eq!(registered["client_name"], name);
    assert_eq!(registered["redirect_uris"], json!([CALLBACK]));
    assert_eq!(registered["token_endpoint_auth_method"], "none");
    registered["client_id"].as_str().unwrap().to_string()
}

/// The query of an authorization request the way a client sends it.
fn authorization_query(client_id: &str) -> String {
    format!(
        "response_type=code&client_id={client_id}&redirect_uri={}&code_challenge={CHALLENGE}&code_challenge_method=S256&state=xyz&scope=&resource={}",
        encode(CALLBACK),
        encode("https://turtle.example.org/mcp"),
    )
}

fn encode(value: &str) -> String {
    value.replace(':', "%3A").replace('/', "%2F")
}

/// The value of a parameter of an address's query, still percent-encoded.
fn parameter(address: &str, name: &str) -> Option<String> {
    let query = address.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}

/// Decodes a percent-encoded value.
fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            decoded.push(u8::from_str_radix(&value[index + 1..index + 3], 16).unwrap());
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).unwrap()
}

/// The consent page's answer, from the page's address as the web app reads it.
fn answer_from(consent_address: &str, allow: bool) -> Value {
    let mut answer = json!({ "allow": allow });
    for name in ["client_id", "redirect_uri", "code_challenge", "code_challenge_method", "state", "scope"] {
        if let Some(value) = parameter(consent_address, name) {
            answer[name] = json!(decode(&value));
        }
    }
    answer
}

async fn exchange(server: &TestServer, client_id: &str, code: &str, verifier: Option<&str>) -> http::Response {
    let mut form = format!("grant_type=authorization_code&code={code}&client_id={client_id}&redirect_uri={}", encode(CALLBACK));
    if let Some(verifier) = verifier {
        form.push_str(&format!("&code_verifier={verifier}"));
    }
    http::request(
        server.addr(),
        "POST",
        "/oauth/token",
        &[("Content-Type", "application/x-www-form-urlencoded")],
        &form,
    )
    .await
}

#[tokio::test]
async fn a_client_registers_a_member_allows_it_and_it_acts_as_the_member() {
    let server = TestServer::start_with_public_url(PUBLIC_URL).await;
    let ada = server.sign_up().await;
    let page = ada.create_page("Roadmap").await;
    let client_id = register(&server, "Claude").await;

    // The browser goes to the authorization endpoint and on to the consent page.
    let authorize = get(&server, &format!("/oauth/authorize?{}", authorization_query(&client_id))).await;
    assert!((300..400).contains(&authorize.status), "{}: {}", authorize.status, authorize.body);
    let consent = authorize.header("location").unwrap().to_string();
    assert!(consent.starts_with("/authorize?"), "{consent}");
    // The consent page is the web app's.
    assert_eq!(get(&server, &consent).await.status, get(&server, "/").await.status);

    // Signed out, it learns nothing, and the web app asks to sign in first.
    let signed_out = http::request(
        server.addr(),
        "POST",
        "/api/describe_connection_request",
        &[("Content-Type", "application/json")],
        &json!({ "client_id": client_id, "redirect_uri": CALLBACK }).to_string(),
    )
    .await;
    assert_eq!(signed_out.status, 403, "{}", signed_out.body);

    let request = ada.call("describe_connection_request", json!({ "client_id": client_id, "redirect_uri": CALLBACK })).await;
    assert_eq!(request, json!({ "app_name": "Claude", "redirect_domain": "claude.ai", "asks_to_change": true }));
    let allowed = ada.call("answer_connection_request", answer_from(&consent, true)).await;
    let redirect = allowed["redirect_to"].as_str().unwrap();
    assert!(redirect.starts_with(CALLBACK), "{redirect}");
    assert_eq!(parameter(redirect, "state").as_deref(), Some("xyz"));
    let code = parameter(redirect, "code").unwrap();

    let issued = exchange(&server, &client_id, &code, Some(VERIFIER)).await;
    assert_eq!(issued.status, 200, "{}", issued.body);
    assert_eq!(issued.header("cache-control"), Some("no-store"));
    let issued = json_of(&issued);
    assert_eq!(issued["token_type"], "Bearer");
    assert_eq!(issued["expires_in"], 3600);
    // A client that names no scope gets what Ada may do.
    assert_eq!(issued["scope"], "turtle:read turtle:write");
    let access_token = issued["access_token"].as_str().unwrap();

    // The client acts as Ada, and her change is credited to her through Claude.
    let write = json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": { "name": "replace_page", "arguments": { "page_id": page, "markdown": "Written by Claude." } },
    });
    let written = mcp(&server, Some(access_token), write).await;
    assert_eq!(written.status, 200, "{}", written.body);
    assert_eq!(json_of(&written)["result"]["isError"], false, "{}", written.body);
    let versions = ada.call("list_versions", json!({ "page_id": page })).await;
    let author = json!([{ "kind": "agent_for_member", "name": "Claude", "member_id": ada.id().await, "member_name": "Ada" }]);
    assert_eq!(versions["versions"][0]["authors"], author);

    // The code was used.
    let again = exchange(&server, &client_id, &code, Some(VERIFIER)).await;
    assert_eq!(again.status, 400);
    assert_eq!(json_of(&again)["error"], "invalid_grant");
}

#[tokio::test]
async fn a_wrong_or_missing_code_verifier_is_refused() {
    let server = TestServer::start_with_public_url(PUBLIC_URL).await;
    let ada = server.sign_up().await;
    let client_id = register(&server, "Claude").await;
    let authorize = get(&server, &format!("/oauth/authorize?{}", authorization_query(&client_id))).await;
    let consent = authorize.header("location").unwrap().to_string();

    let wrong = "x".repeat(43);
    for (verifier, error) in [(Some(wrong.as_str()), "invalid_grant"), (None, "invalid_request")] {
        let allowed = ada.call("answer_connection_request", answer_from(&consent, true)).await;
        let code = parameter(allowed["redirect_to"].as_str().unwrap(), "code").unwrap();
        let refused = exchange(&server, &client_id, &code, verifier).await;
        assert_eq!(refused.status, 400, "{verifier:?}");
        assert_eq!(json_of(&refused)["error"], error, "{verifier:?}");
    }
}

#[tokio::test]
async fn denying_sends_the_client_access_denied() {
    let server = TestServer::start_with_public_url(PUBLIC_URL).await;
    let ada = server.sign_up().await;
    let client_id = register(&server, "Claude").await;
    let authorize = get(&server, &format!("/oauth/authorize?{}", authorization_query(&client_id))).await;
    let consent = authorize.header("location").unwrap().to_string();

    let denied = ada.call("answer_connection_request", answer_from(&consent, false)).await;
    assert_eq!(denied["redirect_to"], format!("{CALLBACK}?error=access_denied&state=xyz"));
}

#[tokio::test]
async fn a_bad_authorization_request_is_refused_without_reaching_the_consent_page() {
    let server = TestServer::start_with_public_url(PUBLIC_URL).await;
    let client_id = register(&server, "Claude").await;

    // An unknown client or another redirect address is never redirected to.
    let unknown = get(&server, &format!("/oauth/authorize?{}", authorization_query("made-up"))).await;
    assert_eq!(unknown.status, 400);
    let elsewhere = authorization_query(&client_id).replace(&encode(CALLBACK), &encode("https://evil.example.com/cb"));
    assert_eq!(get(&server, &format!("/oauth/authorize?{elsewhere}")).await.status, 400);

    // Anything else wrong goes back to the client.
    let plain = authorization_query(&client_id).replace("code_challenge_method=S256", "code_challenge_method=plain");
    let refused = get(&server, &format!("/oauth/authorize?{plain}")).await;
    let location = refused.header("location").unwrap();
    assert!(location.starts_with(CALLBACK), "{location}");
    assert_eq!(parameter(location, "error").as_deref(), Some("invalid_request"));
    assert_eq!(parameter(location, "state").as_deref(), Some("xyz"));
}

/// Calls a tool at the MCP endpoint and says whether it failed.
async fn tool_fails(server: &TestServer, token: &str, name: &str, arguments: Value) -> bool {
    let call = json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": { "name": name, "arguments": arguments } });
    let answered = json_of(&mcp(server, Some(token), call).await);
    answered["result"]["isError"].as_bool().unwrap_or_else(|| panic!("{answered}"))
}

#[tokio::test]
async fn a_member_lowers_a_client_asking_to_read_and_write_to_reading() {
    let server = TestServer::start_with_public_url(PUBLIC_URL).await;
    let ada = server.sign_up().await;
    let page = ada.create_page("Roadmap").await;
    let client_id = register(&server, "Claude").await;
    let query = authorization_query(&client_id).replace("scope=", "scope=turtle%3Aread%20turtle%3Awrite");
    let authorize = get(&server, &format!("/oauth/authorize?{query}")).await;
    let consent = authorize.header("location").unwrap().to_string();

    // The consent page is told what the client asks for.
    let mut answer = answer_from(&consent, true);
    assert_eq!(answer["scope"], "turtle:read turtle:write");
    let request = ada.call("describe_connection_request", json!({ "client_id": client_id, "scope": answer["scope"] })).await;
    assert_eq!(request["asks_to_change"], true);

    answer["only_reading"] = json!(true);
    let allowed = ada.call("answer_connection_request", answer).await;
    let code = parameter(allowed["redirect_to"].as_str().unwrap(), "code").unwrap();
    let issued = json_of(&exchange(&server, &client_id, &code, Some(VERIFIER)).await);
    assert_eq!(issued["scope"], "turtle:read");
    let access_token = issued["access_token"].as_str().unwrap();

    assert!(!tool_fails(&server, access_token, "get_page", json!({ "id": page })).await);
    assert!(tool_fails(&server, access_token, "replace_page", json!({ "page_id": page, "markdown": "Written by Claude." })).await);
}
