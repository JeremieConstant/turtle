//! The HTTP API: every operation of the catalogue under its own name, with
//! JSON input and output (ADR-0005). It adds nothing of its own beyond
//! turning cookies into principals and errors into status codes.

use crate::ops::{self, Catalogue, Core, OpError, Principal, SessionChange};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;

const SESSION_COOKIE: &str = "turtle_session";

struct Api {
    core: Arc<Core>,
    catalogue: Catalogue,
}

pub fn router(core: Arc<Core>) -> Router {
    let api = Arc::new(Api {
        core,
        catalogue: ops::catalogue(),
    });
    Router::new()
        .route("/api/{operation}", post(call))
        .with_state(api)
}

async fn call(
    State(api): State<Arc<Api>>,
    Path(operation): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Response {
    // Only JSON is accepted. A plain HTML form cannot send it, which keeps
    // other sites from posting operations with someone's cookie.
    let is_json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    if !is_json {
        return error_response(OpError::InvalidInput(
            "Send the input as application/json.".into(),
        ));
    }
    let input: Value = if body.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str(&body) {
            Ok(input) => input,
            Err(error) => return error_response(OpError::InvalidInput(error.to_string())),
        }
    };
    // Operations hash passwords and wait for the database, so they run off
    // the async workers.
    let outcome = tokio::task::spawn_blocking(move || {
        let principal = principal(&api.core, &headers)?;
        api.catalogue.call(&api.core, &principal, &operation, input)
    })
    .await;

    match outcome {
        Ok(Ok(reply)) => {
            let mut response = Json(reply.output).into_response();
            let cookie = match reply.session {
                SessionChange::Keep => None,
                SessionChange::Start(secret) => Some(session_cookie(&secret, ops::accounts::SESSION_LIFETIME)),
                SessionChange::End => Some(session_cookie("", 0)),
            };
            if let Some(cookie) = cookie {
                response.headers_mut().insert(header::SET_COOKIE, cookie);
            }
            response
        }
        Ok(Err(error)) => error_response(error),
        Err(error) => error_response(OpError::Internal(error.into())),
    }
}

/// Who is calling, by the session cookie the request carries. Without one,
/// or with one that is unknown or expired, that is nobody.
pub fn principal(core: &Core, headers: &HeaderMap) -> Result<Principal, OpError> {
    match session_secret(headers) {
        Some(secret) => ops::accounts::principal_for_session(core, &secret),
        None => Ok(Principal::Anonymous),
    }
}

fn session_secret(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.to_string())
}

/// The session travels in a cookie scripts cannot read and other sites cannot send.
fn session_cookie(secret: &str, max_age: i64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={secret}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}"
    ))
    .expect("a session secret is plain hex")
}

pub fn error_response(error: OpError) -> Response {
    let status = match &error {
        OpError::NotFound(_) => StatusCode::NOT_FOUND,
        OpError::NotPermitted(_) => StatusCode::FORBIDDEN,
        OpError::InvalidInput(_) => StatusCode::BAD_REQUEST,
        OpError::Conflict(_) => StatusCode::CONFLICT,
        OpError::Internal(cause) => {
            eprintln!("operation failed: {cause:#}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    let body = json!({ "error": { "kind": error.kind(), "message": error.message() } });
    (status, Json(body)).into_response()
}
