//! The HTTP API: every operation of the catalogue under its own name, with
//! JSON input and output (ADR-0005). It adds nothing of its own beyond
//! turning cookies and Tokens into principals and errors into status codes,
//! and taking a ZIP that is uploaded ahead of the import reading it
//! (ADR-0014).

use crate::ops::{self, Catalogue, Core, OpError, Principal, SessionChange};
use axum::extract::rejection::StringRejection;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Extension, Json, Router};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

const SESSION_COOKIE: &str = "turtle_session";

/// How long an upload may go without anything arriving before it is given up.
const UPLOAD_STALLED_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

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
    DefaultBodyLimit::max(base64_input_of(core.attachment_limit()))
}

/// The operations called with a ZIP to import, which may be as large as a
/// ZIP sent in base64 may be.
const IMPORTS: [&str; 5] = ["import", "start_import", "import_confluence", "start_import_confluence", "check_confluence_import"];

/// The largest request an import is called with: a ZIP, in base64. It has a
/// limit of its own, far above the rest, which is checked before anyone is
/// known; only someone signed in gets as far as the import. A larger ZIP is
/// uploaded ahead of the import (`upload`).
fn import_input_limit(core: &Core) -> DefaultBodyLimit {
    DefaultBodyLimit::max(base64_input_of(ops::import::sent_limit(core)))
}

/// The size of an input carrying a file of `bytes` bytes in base64.
fn base64_input_of(bytes: u64) -> usize {
    (bytes.div_ceil(3) * 4 + 64 * 1024) as usize
}

pub fn router(core: Arc<Core>, client_address_header: Option<HeaderName>) -> Router {
    let limit = input_limit(&core);
    let import_limit = import_input_limit(&core);
    let api = Arc::new(Api {
        core,
        catalogue: ops::catalogue(),
        client_address_header,
    });
    // The limit given with a route takes the place of the one around them all.
    let mut router = Router::new();
    for operation in IMPORTS {
        router = router.route(&format!("/api/{operation}"), post(call_import).layer(import_limit));
    }
    router
        // The upload counts what arrives itself, against the import limit.
        .route("/api/uploads", post(upload).layer(DefaultBodyLimit::disable()))
        .route("/api/{operation}", post(call))
        .layer(limit)
        .with_state(api)
}

async fn call(
    State(api): State<Arc<Api>>,
    Path(operation): Path<String>,
    connection: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    body: Result<String, StringRejection>,
) -> Response {
    call_operation(api, operation, connection, headers, body).await
}

/// An import, named by the last part of its route, which may be larger
/// than other requests.
async fn call_import(
    State(api): State<Arc<Api>>,
    uri: Uri,
    connection: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    body: Result<String, StringRejection>,
) -> Response {
    let operation = uri.path().rsplit('/').next().unwrap_or_default().to_string();
    call_operation(api, operation, connection, headers, body).await
}

/// Takes a ZIP ahead of the import that reads it (ADR-0014): the body of the
/// request, written to a file as it arrives and never held whole. Answers
/// with the `upload_id` to give the import in place of `content`.
async fn upload(State(api): State<Arc<Api>>, headers: HeaderMap, body: axum::body::Body) -> Response {
    // Only a ZIP, or bytes of no stated kind. A form on another site can
    // send neither, which keeps other sites from uploading with someone's
    // cookie.
    let kind = headers.get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()).unwrap_or_default();
    let kind = kind.split(';').next().unwrap_or_default().trim();
    if !kind.eq_ignore_ascii_case("application/zip") && !kind.eq_ignore_ascii_case("application/octet-stream") {
        return error_response(OpError::InvalidInput("Send the ZIP as the body of the request, as application/zip.".into()));
    }
    let announced = headers.get(header::CONTENT_LENGTH).and_then(|value| value.to_str().ok()).and_then(|value| value.parse().ok());
    // Who is uploading is found out before any of the ZIP is taken.
    let core = api.core.clone();
    let begun = tokio::task::spawn_blocking(move || {
        let principal = principal(&core, &headers)?;
        ops::import::receive_upload(&core, &principal, announced).map_err(Refusal::Op)
    })
    .await;
    let mut receiving = match begun {
        Ok(Ok(receiving)) => receiving,
        Ok(Err(refused)) => return refused.into_response(),
        Err(error) => return error_response(OpError::Internal(error.into())),
    };
    let mut pieces = body.into_data_stream();
    loop {
        // An upload that stops arriving is given up, so it does not keep its
        // sender from uploading again. What arrived so far is removed.
        let broke_off = || error_response(OpError::InvalidInput("The upload broke off before all of the ZIP arrived. Try again.".into()));
        let piece = match tokio::time::timeout(UPLOAD_STALLED_AFTER, pieces.next()).await {
            Ok(Some(Ok(piece))) => piece,
            Ok(None) => break,
            Ok(Some(Err(_))) | Err(_) => return broke_off(),
        };
        let core = api.core.clone();
        // Writing a file waits for the disk, so it happens off the async workers.
        let written = tokio::task::spawn_blocking(move || receiving.write(&core, &piece).map(|()| receiving)).await;
        receiving = match written {
            Ok(Ok(receiving)) => receiving,
            Ok(Err(refused)) => return error_response(refused),
            Err(error) => return error_response(OpError::Internal(error.into())),
        };
    }
    let core = api.core.clone();
    match tokio::task::spawn_blocking(move || receiving.finish(&core)).await {
        Ok(Ok(uploaded)) => Json(uploaded).into_response(),
        Ok(Err(refused)) => error_response(refused),
        Err(error) => error_response(OpError::Internal(error.into())),
    }
}

async fn call_operation(
    api: Arc<Api>,
    operation: String,
    connection: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    body: Result<String, StringRejection>,
) -> Response {
    let body = match body {
        Ok(body) => body,
        Err(rejection) if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            let message = if IMPORTS.contains(&operation.as_str()) {
                ops::import::too_large_to_send(&api.core)
            } else {
                OpError::InvalidInput("This request is larger than Turtle accepts. Send less at once.".into())
            };
            let mut response = error_response(message);
            *response.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
            return response;
        }
        Err(rejection) => return rejection.into_response(),
    };
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
    // The input holds what the body did; an import's ZIP need not be kept twice.
    drop(body);
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
