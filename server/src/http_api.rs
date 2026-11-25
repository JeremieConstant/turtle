//! The HTTP API: every operation of the catalogue under its own name, with
//! JSON input and output (ADR-0005). It adds nothing of its own beyond
//! turning cookies and Tokens into principals and errors into status codes.

use crate::ops::{self, Catalogue, Core, OpError, Principal, SessionChange};
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Extension, Json, Router};
use serde_json::{json, Value};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

const SESSION_COOKIE: &str = "turtle_session";

struct Api {
    core: Arc<Core>,
    catalogue: Catalogue,
    /// The header a reverse proxy passes the client's address on in, when
    /// the operator says Turtle is behind one.
    client_address_header: Option<HeaderName>,
}

/// The largest request an operation is called with. An attached file
/// arrives in base64, a third larger than the file, with room for the rest
/// of the input.
pub fn input_limit(core: &Core) -> DefaultBodyLimit {
    DefaultBodyLimit::max((core.attachment_limit().div_ceil(3) * 4 + 64 * 1024) as usize)
}

pub fn router(core: Arc<Core>, client_address_header: Option<HeaderName>) -> Router {
    let limit = input_limit(&core);
    let api = Arc::new(Api {
        core,
        catalogue: ops::catalogue(),
        client_address_header,
    });
    Router::new()
        .route("/api/{operation}", post(call))
        .layer(limit)
        .with_state(api)
}

async fn call(
    State(api): State<Arc<Api>>,
    Path(operation): Path<String>,
    connection: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let connected = connection.map(|Extension(ConnectInfo(address))| address.ip());
    let client = client_address(api.client_address_header.as_ref(), &headers).or(connected);
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
        api.catalogue
            .call_from(&api.core, &principal, client, &operation, input)
            .map_err(Refusal::Op)
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
        Ok(Err(refused)) => refused.into_response(),
        Err(error) => error_response(OpError::Internal(error.into())),
    }
}

/// The client's address as the reverse proxy passed it on in `header`: the
/// last address in it, the one the proxy added, since a client can put
/// anything before it. Some proxies add the client's port. None without a
/// proxy, or if the header holds no address.
fn client_address(header: Option<&HeaderName>, headers: &HeaderMap) -> Option<IpAddr> {
    let value = headers.get_all(header?).iter().next_back()?.to_str().ok()?;
    let last = value.rsplit(',').next()?.trim();
    last.parse()
        .or_else(|_| last.parse::<SocketAddr>().map(|with_port| with_port.ip()))
        .ok()
}

/// Why a request is refused.
pub enum Refusal {
    /// The request carries a Token that is unknown, expired or revoked, or
    /// whose owner was deactivated.
    InvalidToken,
    Op(OpError),
}

impl Refusal {
    pub fn into_response(self) -> Response {
        match self {
            Refusal::InvalidToken => unauthorized(
                "This Token is not valid: it is unknown, has expired or was revoked. Create a new one in Turtle.",
            ),
            Refusal::Op(error) => error_response(error),
        }
    }
}

/// Who is calling. A Token in the `Authorization` header wins over the
/// session cookie; without either, or with a session that is unknown or
/// expired, that is nobody.
pub fn principal(core: &Core, headers: &HeaderMap) -> Result<Principal, Refusal> {
    if let Some(token) = bearer_token(headers) {
        return principal_for_bearer(core, token);
    }
    match session_secret(headers) {
        Some(secret) => ops::accounts::principal_for_session(core, &secret).map_err(Refusal::Op),
        None => Ok(Principal::Anonymous),
    }
}

/// Who acts through a Token value; refused if it is not valid.
pub fn principal_for_bearer(core: &Core, token: &str) -> Result<Principal, Refusal> {
    match ops::tokens::principal_for_token(core, token) {
        Ok(Some(principal)) => Ok(principal),
        Ok(None) => Err(Refusal::InvalidToken),
        Err(error) => Err(Refusal::Op(error)),
    }
}

/// The Token value of an `Authorization: Bearer` header, if there is one.
pub fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.trim().split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
}

/// 401 with the same body as any other refusal, telling the client to
/// authenticate with a Token.
pub fn unauthorized(message: &str) -> Response {
    let body = json!({ "error": { "kind": "not_permitted", "message": message } });
    let mut response = (StatusCode::UNAUTHORIZED, Json(body)).into_response();
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
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
