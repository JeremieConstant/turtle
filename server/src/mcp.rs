//! The MCP endpoint (ADR-0005): every operation of the catalogue that is not
//! browser-only as one MCP tool, over Streamable HTTP at `/mcp`, for an Agent
//! holding a Token. Like the HTTP API it adds nothing of its own beyond
//! turning a Token into a principal and outcomes into JSON-RPC.
//!
//! The endpoint keeps no sessions and opens no event streams: every request
//! is answered with one JSON body, which Streamable HTTP allows.

use crate::http_api::{bearer_token, error_response, principal_for_bearer, unauthorized, Refusal};
use crate::ops::{self, Catalogue, Core, OpError, Principal};
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;

/// The protocol versions this endpoint speaks, newest first.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];

const INSTRUCTIONS: &str = "Turtle is a knowledge base of Areas, each holding a tree of Pages and Folders. \
Every tool acts as the Member who owns your Token, with their Roles, reduced by the Token's limits. \
Something you may not read is reported as not found. \
Pages are read and written as Markdown: whole, one Section by its heading, or by appending. \
People editing the Page keep their work and see your change with your Token's name, and each write is its own Version.";

// JSON-RPC error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

struct Mcp {
    core: Arc<Core>,
    catalogue: Catalogue,
}

pub fn router(core: Arc<Core>) -> Router {
    let limit = crate::http_api::input_limit(&core);
    let mcp = Arc::new(Mcp {
        core,
        catalogue: ops::catalogue(),
    });
    // GET would open an event stream and DELETE would end a session; this
    // endpoint has neither, so both are answered with 405.
    Router::new().route("/mcp", post(receive)).layer(limit).with_state(mcp)
}

async fn receive(State(mcp): State<Arc<Mcp>>, headers: HeaderMap, body: String) -> Response {
    // A page of another site is refused even before its Token is looked at.
    // DNS rebinding can make Origin and Host agree, so the real guard is the
    // Token, which such a page does not have.
    if !same_origin(&headers) {
        return error_response(OpError::NotPermitted("Requests from other sites are not accepted.".into()));
    }
    if let Some(version) = headers.get("mcp-protocol-version").and_then(|value| value.to_str().ok()) {
        if !PROTOCOL_VERSIONS.contains(&version) {
            return error_response(OpError::InvalidInput(format!(
                "Turtle does not speak MCP protocol version {version}."
            )));
        }
    }
    let Some(token) = bearer_token(&headers).map(str::to_string) else {
        let mut refused = unauthorized("Send a Token as Authorization: Bearer <token>. A Member creates one in Turtle.");
        refused.headers_mut().insert(header::WWW_AUTHENTICATE, crate::oauth::challenge(&mcp.core, false));
        return refused;
    };
    let message: Value = match serde_json::from_str(&body) {
        Ok(message) => message,
        Err(_) => {
            let answer = failure(Value::Null, PARSE_ERROR, "The request is not valid JSON.");
            return (StatusCode::BAD_REQUEST, Json(answer)).into_response();
        }
    };

    // Operations wait for the database and may hash, so they run off the
    // async workers, as in the HTTP API.
    let core = mcp.core.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let principal = principal_for_bearer(&mcp.core, &token)?;
        Ok::<_, Refusal>(answer(&mcp.core, &mcp.catalogue, &principal, message))
    })
    .await;

    match outcome {
        Ok(Ok(Some(answer))) => Json(answer).into_response(),
        // A notification or a response arrived, which gets no answer.
        Ok(Ok(None)) => StatusCode::ACCEPTED.into_response(),
        Ok(Err(Refusal::InvalidToken)) => {
            let mut refused = Refusal::InvalidToken.into_response();
            refused.headers_mut().insert(header::WWW_AUTHENTICATE, crate::oauth::challenge(&core, true));
            refused
        }
        Ok(Err(refused)) => refused.into_response(),
        Err(error) => error_response(OpError::Internal(error.into())),
    }
}

/// Whether a request either comes from no browser page at all or from a page
/// served by this same Turtle.
fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|value| value.to_str().ok()) else {
        return true;
    };
    let host = headers.get(header::HOST).and_then(|value| value.to_str().ok());
    let origin_host = origin.split_once("://").map(|(_, rest)| rest.trim_end_matches('/'));
    origin_host.is_some() && origin_host == host
}

/// The answer to one JSON-RPC message, or `None` for a notification or a
/// response, which are not answered.
fn answer(core: &Core, catalogue: &Catalogue, principal: &Principal, message: Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // A response to something Turtle asked is not answered; anything
        // else without a method is not a JSON-RPC message Turtle accepts.
        let is_response = message.get("result").is_some() || message.get("error").is_some();
        return (!is_response).then(|| {
            failure(id.unwrap_or(Value::Null), INVALID_REQUEST, "Send one JSON-RPC request or notification.")
        });
    };
    // A notification is not answered.
    let id = id?;
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    let outcome = match method {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools(catalogue) })),
        "tools/call" => call_tool(core, catalogue, principal, &params),
        _ => Err((METHOD_NOT_FOUND, format!("Turtle has no method {method}."))),
    };
    Some(match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => failure(id, code, &message),
    })
}

fn failure(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = requested
        .filter(|requested| PROTOCOL_VERSIONS.contains(requested))
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "turtle", "title": "Turtle", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

/// One tool for every operation that is not browser-only.
fn tools(catalogue: &Catalogue) -> Vec<Value> {
    catalogue
        .operations()
        .iter()
        .filter(|operation| !operation.browser_only)
        .map(|operation| {
            let mut tool = json!({
                "name": operation.name,
                "description": operation.description,
                "inputSchema": operation.input_schema,
                "annotations": { "readOnlyHint": !operation.changes },
            });
            if is_object_schema(&operation.output_schema) {
                tool["outputSchema"] = operation.output_schema.clone();
            }
            tool
        })
        .collect()
}

/// MCP only takes output schemas that describe an object.
fn is_object_schema(schema: &Value) -> bool {
    schema.get("type") == Some(&json!("object"))
}

fn call_tool(core: &Core, catalogue: &Catalogue, principal: &Principal, params: &Value) -> Result<Value, (i64, String)> {
    let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
    let operation = catalogue
        .operations()
        .iter()
        .find(|operation| operation.name == name && !operation.browser_only)
        .ok_or_else(|| (INVALID_PARAMS, format!("Turtle has no tool named {name}.")))?;
    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    // An operation that does not happen is the tool's result, so the Agent
    // reads the message and can act on it.
    Ok(match catalogue.call(core, principal, name, arguments) {
        Ok(reply) => {
            let text = serde_json::to_string_pretty(&reply.output).unwrap_or_default();
            let mut result = json!({ "content": [{ "type": "text", "text": text }], "isError": false });
            if is_object_schema(&operation.output_schema) {
                result["structuredContent"] = reply.output;
            }
            result
        }
        Err(error) => {
            if let OpError::Internal(cause) = &error {
                eprintln!("operation {name} failed: {cause:#}");
            }
            json!({ "content": [{ "type": "text", "text": error.message() }], "isError": true })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The operations that only make sense in a browser, so they have no MCP
    /// tool. Written out on purpose: an operation gets onto this list by a
    /// deliberate change here, never by accident.
    const BROWSER_ONLY: [&str; 12] = [
        "create_first_account",
        "sign_in",
        "sign_out",
        "accept_invitation",
        "reset_password",
        "create_token",
        "list_tokens",
        "revoke_token",
        "describe_connecting",
        "set_connecting_ai",
        "describe_connection_request",
        "answer_connection_request",
    ];

    #[test]
    fn every_operation_has_a_tool_unless_it_is_a_listed_browser_only_exception() {
        let catalogue = ops::catalogue();
        let tools = tools(&catalogue);
        let tool_names: Vec<&str> = tools.iter().map(|tool| tool["name"].as_str().unwrap()).collect();

        for operation in catalogue.operations() {
            let has_tool = tool_names.contains(&operation.name);
            let excepted = BROWSER_ONLY.contains(&operation.name);
            assert!(has_tool != excepted, "{}: tool {has_tool}, browser-only exception {excepted}", operation.name);
        }
        for tool in &tools {
            assert_eq!(tool["inputSchema"]["type"], "object", "{}", tool["name"]);
            assert!(!tool["description"].as_str().unwrap().is_empty(), "{}", tool["name"]);
        }
    }
}
