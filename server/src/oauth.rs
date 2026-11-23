//! OAuth for AI clients (ADR-0010), spoken the way the MCP authorization
//! specification asks: protected resource and authorization server metadata,
//! open Dynamic Client Registration, and the authorization code flow with
//! PKCE. It is offered only when `TURTLE_PUBLIC_URL` says where Turtle is
//! reached, since that address is the issuer and the base of every other.
//!
//! Like the other adapters it decides nothing: `ops::connections` does. The
//! consent page is a screen of the web app, which the authorization endpoint
//! sends the browser to. The page asks Turtle who is signed in with a request
//! of its own, which carries the session cookie even though the browser came
//! from the client's site, where a `SameSite=Strict` cookie stays behind.

use crate::ops::connections::{self, with_query, CodeExchange, GrantRefusal, NewClient};
use crate::ops::{Core, OpError};
use axum::extract::rejection::FormRejection;
use axum::extract::{Form, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Where the protected resource metadata of the MCP endpoint is (RFC 9728:
/// the well-known name, then the resource's path).
pub const RESOURCE_METADATA_PATH: &str = "/.well-known/oauth-protected-resource/mcp";

/// Where the web app shows the consent page.
const CONSENT_SCREEN: &str = "/authorize";

/// Reads `TURTLE_PUBLIC_URL`: the http or https address of Turtle's root.
/// Turtle is served at the root of its host, so the address has no path; a
/// trailing slash is dropped, so every OAuth address is the public URL
/// followed by its path.
pub fn public_url(value: &str) -> anyhow::Result<String> {
    let url = value.trim().trim_end_matches('/');
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"));
    let valid = rest.is_some_and(|host| {
        !host.is_empty() && !host.contains(['/', '?', '#', '@']) && !host.chars().any(char::is_whitespace)
    });
    anyhow::ensure!(
        valid,
        "TURTLE_PUBLIC_URL must be the http or https address Turtle is reached at, such as https://turtle.example.org, not {value:?}"
    );
    Ok(url.to_string())
}

/// The OAuth routes, or none when Turtle does not know its public address.
pub fn router(core: Arc<Core>) -> Router {
    if core.public_url().is_none() {
        return Router::new();
    }
    Router::new()
        .route("/.well-known/oauth-protected-resource", get(resource_metadata).options(preflight))
        .route(RESOURCE_METADATA_PATH, get(resource_metadata).options(preflight))
        .route("/.well-known/oauth-authorization-server", get(server_metadata).options(preflight))
        .route("/oauth/register", post(register).options(preflight))
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/token", post(token).options(preflight))
        .with_state(core)
}

/// The `WWW-Authenticate` challenge of the MCP endpoint: a client that may
/// sign in with OAuth is told where to start.
pub fn challenge(core: &Core, invalid_token: bool) -> HeaderValue {
    let Some(base) = core.public_url() else { return HeaderValue::from_static("Bearer") };
    let error = if invalid_token { r#"error="invalid_token", "# } else { "" };
    HeaderValue::from_str(&format!(r#"Bearer {error}resource_metadata="{base}{RESOURCE_METADATA_PATH}""#))
        .unwrap_or_else(|_| HeaderValue::from_static("Bearer"))
}

fn base(core: &Core) -> &str {
    core.public_url().unwrap_or_default()
}

async fn resource_metadata(State(core): State<Arc<Core>>) -> Response {
    let base = base(&core);
    open_to_any_site(Json(json!({
        "resource": format!("{base}/mcp"),
        "authorization_servers": [base],
        "bearer_methods_supported": ["header"],
        "resource_name": "Turtle",
    })))
}

async fn server_metadata(State(core): State<Arc<Core>>) -> Response {
    let base = base(&core);
    open_to_any_site(Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
    })))
}

/// Clients that run in a browser, such as an MCP inspector, read the
/// metadata, register and exchange codes from another site. None of these
/// addresses acts on a cookie, so any site may.
fn open_to_any_site(response: impl IntoResponse) -> Response {
    let mut response = response.into_response();
    response.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    response
}

async fn preflight() -> Response {
    let mut response = open_to_any_site(StatusCode::NO_CONTENT);
    let headers = response.headers_mut();
    headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, POST, OPTIONS"));
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Content-Type, Authorization, MCP-Protocol-Version"),
    );
    response
}

/// An OAuth error answer: `{"error", "error_description"}`.
fn oauth_error(status: StatusCode, error: &str, description: &str) -> Response {
    let mut response = open_to_any_site((status, Json(json!({ "error": error, "error_description": description }))));
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Logs a failure of Turtle itself and answers like OAuth does.
fn server_error(error: OpError) -> Response {
    if let OpError::Internal(cause) = &error {
        eprintln!("OAuth request failed: {cause:#}");
    }
    oauth_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "Turtle could not answer. Try again.")
}

#[derive(Deserialize)]
struct Registration {
    #[serde(default)]
    redirect_uris: Vec<String>,
    #[serde(default)]
    client_name: Option<String>,
}

/// Dynamic Client Registration (RFC 7591). Every client is public: it
/// proves itself with PKCE, never with a secret.
async fn register(State(core): State<Arc<Core>>, body: String) -> Response {
    let Ok(registration) = serde_json::from_str::<Registration>(&body) else {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_client_metadata", "Send the client metadata as JSON.");
    };
    let input = NewClient { redirect_uris: registration.redirect_uris, client_name: registration.client_name };
    let registered = tokio::task::spawn_blocking(move || connections::register_client(&core, input)).await;
    match registered {
        Ok(Ok(client)) => open_to_any_site((
            StatusCode::CREATED,
            Json(json!({
                "client_id": client.id,
                "client_id_issued_at": client.created_at,
                "client_name": client.name,
                "redirect_uris": client.redirect_uris,
                "grant_types": ["authorization_code"],
                "response_types": ["code"],
                "token_endpoint_auth_method": "none",
            })),
        )),
        Ok(Err(OpError::InvalidInput(message))) => oauth_error(StatusCode::BAD_REQUEST, "invalid_redirect_uri", &message),
        Ok(Err(error)) => server_error(error),
        Err(error) => server_error(OpError::Internal(error.into())),
    }
}

/// The authorization endpoint. A request that names no registered client or
/// none of its redirect addresses is answered here, never sent anywhere;
/// anything else wrong is sent back to the client. A good request goes on to
/// the consent page, which signs the Member in first if need be.
async fn authorize(State(core): State<Arc<Core>>, Query(query): Query<HashMap<String, String>>) -> Response {
    let client_id = query.get("client_id").cloned().unwrap_or_default();
    let redirect_uri = query.get("redirect_uri").cloned();
    let checking = core.clone();
    let checked = tokio::task::spawn_blocking(move || {
        connections::requesting_client(&checking, &client_id, redirect_uri.as_deref())
    })
    .await;
    let redirect_uri = match checked {
        Ok(Ok((_, redirect_uri))) => redirect_uri,
        Ok(Err(OpError::NotFound(message))) => return (StatusCode::BAD_REQUEST, message).into_response(),
        Ok(Err(error)) => return server_error(error),
        Err(error) => return server_error(OpError::Internal(error.into())),
    };
    let state = query.get("state").map(String::as_str);
    let back = |error: &str, description: &str| {
        let answer = [("error", Some(error)), ("error_description", Some(description)), ("state", state)];
        Redirect::to(&with_query(&redirect_uri, &answer)).into_response()
    };
    if query.get("response_type").map(String::as_str) != Some("code") {
        return back("unsupported_response_type", "Only the authorization code flow is supported.");
    }
    let challenge = query.get("code_challenge").map(String::as_str);
    let method = query.get("code_challenge_method").map(String::as_str);
    if let Err(description) = connections::pkce_challenge(challenge, method) {
        return back("invalid_request", description);
    }

    // The consent page needs what the answer is made of; the rest of the
    // request (scope, resource) asks for nothing Turtle can limit yet.
    let mut forwarded = vec![("client_id", query.get("client_id").map(String::as_str)), ("redirect_uri", Some(redirect_uri.as_str()))];
    for name in ["code_challenge", "code_challenge_method", "state"] {
        forwarded.push((name, query.get(name).map(String::as_str)));
    }
    Redirect::to(&with_query(CONSENT_SCREEN, &forwarded)).into_response()
}

/// The token endpoint: exchanges an authorization code, form-encoded as
/// OAuth sends it, for an access token.
async fn token(State(core): State<Arc<Core>>, form: Result<Form<HashMap<String, String>>, FormRejection>) -> Response {
    let Ok(Form(form)) = form else {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_request", "Send the request form-encoded.");
    };
    match form.get("grant_type").map(String::as_str) {
        Some("authorization_code") => {}
        Some(_) => return oauth_error(StatusCode::BAD_REQUEST, "unsupported_grant_type", "Only authorization codes are exchanged."),
        None => return oauth_error(StatusCode::BAD_REQUEST, "invalid_request", "Send a grant_type."),
    }
    let field = |name: &str| form.get(name).filter(|value| !value.is_empty()).cloned();
    let (Some(code), Some(client_id), Some(code_verifier)) = (field("code"), field("client_id"), field("code_verifier")) else {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_request", "Send the code, client_id and code_verifier.");
    };
    let input = CodeExchange { code, client_id, redirect_uri: field("redirect_uri"), code_verifier };
    let exchanged = tokio::task::spawn_blocking(move || connections::exchange_code(&core, input)).await;
    match exchanged {
        Ok(Ok(issued)) => {
            let body: Value = json!({
                "access_token": issued.access_token,
                "token_type": "Bearer",
                "expires_in": issued.expires_in,
            });
            let mut response = open_to_any_site(Json(body));
            response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Ok(Err(GrantRefusal::InvalidGrant)) => oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "The code is unknown, used or expired, or the code_verifier does not match it.",
        ),
        Ok(Err(GrantRefusal::Failed(error))) => server_error(error),
        Err(error) => server_error(OpError::Internal(error.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_public_url_is_an_http_address_without_a_trailing_slash() {
        assert_eq!(public_url("https://turtle.example.org/").unwrap(), "https://turtle.example.org");
        assert_eq!(public_url(" http://localhost:4100 ").unwrap(), "http://localhost:4100");
        for invalid in [
            "turtle.example.org",
            "ftp://example.org",
            "https://",
            "https://example.org/turtle",
            "https://example.org/?a=1",
            "https://example.org/#x",
        ] {
            assert!(public_url(invalid).is_err(), "{invalid}");
        }
    }
}
