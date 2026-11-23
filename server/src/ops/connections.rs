//! Connections: a Member allows an AI client, registered through OAuth, to
//! act as them (ADR-0010). A connection is a Token of the Member, named
//! after the client, so it never has more access than the Member and its
//! changes are credited like any Token's. The client holds only access
//! tokens for it, which last an hour.
//!
//! OAuth itself (metadata, registration, the authorization and token
//! endpoints) is spoken by `crate::oauth`; what is decided is decided here.

use super::accounts::{digest, random_hex};
use super::{Catalogue, Core, OpError, OpResult, Principal};
use base64::Engine;
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(super) fn register(catalogue: &mut Catalogue) {
    {
        let operation = catalogue.add(
            "describe_connection_request",
            "Says which app asks to act as the caller, and where Turtle sends the answer, so the caller can decide whether to allow it.",
            describe_connection_request,
        );
        operation.browser_only = true;
        operation.changes = false;
    }
    catalogue
        .add(
            "answer_connection_request",
            "Allows an app to act as the caller, or denies it, and returns the address to send the browser back to the app with the answer.",
            answer_connection_request,
        )
        .browser_only = true;
}

/// How long an access token works, in seconds.
pub const ACCESS_TOKEN_LIFETIME: i64 = 60 * 60;

/// How long an authorization code can be exchanged, in seconds: long enough
/// for a client to do it right away, too short to be worth stealing.
const CODE_LIFETIME: i64 = 5 * 60;

/// Enough for a client with a few callbacks; more is no client's need.
const MAX_REDIRECT_URIS: usize = 10;

/// Long enough for any app's name, short enough to fit a list.
const MAX_NAME_LENGTH: usize = 200;

/// Access token values start like a Token's, so a leaked one is as easy to
/// recognise.
const ACCESS_TOKEN_PREFIX: &str = "turtle_";

/// What a client says about itself when it registers (RFC 7591).
#[derive(Debug, Default, Deserialize)]
pub struct NewClient {
    #[serde(default)]
    pub redirect_uris: Vec<String>,
    #[serde(default)]
    pub client_name: Option<String>,
}

/// A registered client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub redirect_uris: Vec<String>,
    pub created_at: i64,
}

/// Registers a client. Anyone may (ADR-0010): what keeps a client from
/// acting is the Member who has to allow it, seeing its name and where it
/// sends the answer.
pub fn register_client(core: &Core, input: NewClient) -> Result<Client, OpError> {
    require_oauth(core)?;
    if input.redirect_uris.is_empty() {
        return Err(OpError::InvalidInput("Register at least one redirect address.".into()));
    }
    if input.redirect_uris.len() > MAX_REDIRECT_URIS {
        return Err(OpError::InvalidInput(format!("Register at most {MAX_REDIRECT_URIS} redirect addresses.")));
    }
    for uri in &input.redirect_uris {
        check_redirect_uri(uri)?;
    }
    let name = match input.client_name.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => name.chars().take(MAX_NAME_LENGTH).collect(),
        // Without a name of its own, the client is known by where it sends
        // the answer, which is what the Member is shown anyway.
        _ => redirect_domain(&input.redirect_uris[0]).to_string(),
    };
    let client = Client {
        id: random_hex(16)?,
        name,
        redirect_uris: input.redirect_uris,
        created_at: core.now(),
    };
    let uris = serde_json::to_string(&client.redirect_uris).map_err(anyhow::Error::from)?;
    core.db().execute(
        "INSERT INTO oauth_clients (id, name, redirect_uris, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![client.id, client.name, uris, client.created_at],
    )?;
    Ok(client)
}

/// A redirect address must reach the client and nobody else: an `https`
/// address, or `http` back to the computer the client runs on, as a desktop
/// app or Claude Code listens there. A fragment cannot carry an answer.
fn check_redirect_uri(uri: &str) -> Result<(), OpError> {
    let invalid = || OpError::InvalidInput(format!("{uri} cannot be a redirect address. Use an https address."));
    if uri.contains('#') || uri.chars().any(char::is_whitespace) {
        return Err(invalid());
    }
    let (scheme, _) = uri.split_once("://").ok_or_else(invalid)?;
    let host = host_of(uri);
    let loopback = matches!(host, "localhost" | "127.0.0.1" | "[::1]");
    match scheme {
        "https" if !host.is_empty() => Ok(()),
        "http" if loopback => Ok(()),
        _ => Err(invalid()),
    }
}

/// The host of an address, without port or user, as it is shown to a Member.
fn host_of(uri: &str) -> &str {
    let rest = uri.split_once("://").map_or("", |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    if host.starts_with('[') {
        // An IPv6 address keeps its brackets; the port follows them.
        return host.split_inclusive(']').next().unwrap_or(host);
    }
    host.split(':').next().unwrap_or_default()
}

/// The domain the answer goes to, which the Member is shown so a client that
/// only calls itself Claude is recognisable.
fn redirect_domain(uri: &str) -> &str {
    host_of(uri)
}

/// The client asking, if it is registered and the redirect address is one
/// of its own. Without a redirect address, its only one is meant.
pub fn requesting_client(core: &Core, client_id: &str, redirect_uri: Option<&str>) -> Result<(Client, String), OpError> {
    require_oauth(core)?;
    let invalid = || OpError::NotFound("This connection request is not valid. Start connecting again from the app.".into());
    let client = client(&core.db(), client_id)?.ok_or_else(invalid)?;
    let redirect_uri = match redirect_uri {
        Some(uri) if client.redirect_uris.iter().any(|known| known == uri) => uri.to_string(),
        None if client.redirect_uris.len() == 1 => client.redirect_uris[0].clone(),
        _ => return Err(invalid()),
    };
    Ok((client, redirect_uri))
}

fn client(db: &Connection, id: &str) -> Result<Option<Client>, OpError> {
    let found = db
        .query_row(
            "SELECT id, name, redirect_uris, created_at FROM oauth_clients WHERE id = ?1",
            [id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?)),
        )
        .optional()?;
    let Some((id, name, uris, created_at)) = found else { return Ok(None) };
    let redirect_uris = serde_json::from_str(&uris).map_err(anyhow::Error::from)?;
    Ok(Some(Client { id, name, redirect_uris, created_at }))
}

/// OAuth is offered only where Turtle knows its own address (ADR-0010).
/// Where it is not, nothing of it exists.
fn require_oauth(core: &Core) -> Result<(), OpError> {
    match core.public_url() {
        Some(_) => Ok(()),
        None => Err(OpError::NotFound("This Turtle does not offer connecting apps. Ask a Workspace Admin to set its public address.".into())),
    }
}

/// The Member who may decide on a connection: signed in to Turtle in a
/// browser, not acting through a Token. Turtle has no Guests yet; when it
/// does, they are refused here, since only Members may connect an app.
fn deciding_member(principal: &Principal) -> Result<&str, OpError> {
    match principal {
        Principal::Member { id, token: None, .. } => Ok(id),
        Principal::Member { token: Some(_), .. } | Principal::Agent { .. } => Err(OpError::NotPermitted(
            "Only a Member signed in to Turtle in a browser can allow an app to connect.".into(),
        )),
        Principal::Anonymous => Err(OpError::NotPermitted("Sign in first.".into())),
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RequestedConnection {
    /// The app's client ID, as it sent it.
    pub client_id: String,
    /// Where the app wants the answer, as it sent it.
    #[serde(default)]
    pub redirect_uri: Option<String>,
}

/// An app asking to act as the caller.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ConnectionRequest {
    /// The name the app registered with. Any app can choose any name.
    pub app_name: String,
    /// The domain the answer is sent to, which tells who really asks.
    pub redirect_domain: String,
}

pub fn describe_connection_request(core: &Core, principal: &Principal, input: RequestedConnection) -> OpResult<ConnectionRequest> {
    deciding_member(principal)?;
    let (client, redirect_uri) = requesting_client(core, &input.client_id, input.redirect_uri.as_deref())?;
    Ok(ConnectionRequest {
        app_name: client.name,
        redirect_domain: redirect_domain(&redirect_uri).to_string(),
    }
    .into())
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConnectionAnswer {
    /// The app's client ID, as it sent it.
    pub client_id: String,
    /// Where the app wants the answer, as it sent it.
    #[serde(default)]
    pub redirect_uri: Option<String>,
    /// The PKCE code challenge the app sent.
    #[serde(default)]
    pub code_challenge: Option<String>,
    /// The PKCE method the app sent; only S256 is accepted.
    #[serde(default)]
    pub code_challenge_method: Option<String>,
    /// What the app sent to recognise the answer, given back unchanged.
    #[serde(default)]
    pub state: Option<String>,
    /// Whether the caller allows the app to act as them.
    pub allow: bool,
}

/// Where to send the browser, with the answer for the app.
#[derive(Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ConnectionRedirect {
    pub redirect_to: String,
}

pub fn answer_connection_request(core: &Core, principal: &Principal, input: ConnectionAnswer) -> OpResult<ConnectionRedirect> {
    let member_id = deciding_member(principal)?;
    let (client, redirect_uri) = requesting_client(core, &input.client_id, input.redirect_uri.as_deref())?;
    let state = input.state.as_deref();
    if !input.allow {
        let redirect_to = with_query(&redirect_uri, &[("error", Some("access_denied")), ("state", state)]);
        return Ok(ConnectionRedirect { redirect_to }.into());
    }
    let challenge = match pkce_challenge(input.code_challenge.as_deref(), input.code_challenge_method.as_deref()) {
        Ok(challenge) => challenge,
        Err(description) => {
            let answer = [("error", Some("invalid_request")), ("error_description", Some(description)), ("state", state)];
            return Ok(ConnectionRedirect { redirect_to: with_query(&redirect_uri, &answer) }.into());
        }
    };
    let code = random_hex(32)?;
    let now = core.now();
    let db = core.db();
    // Codes nobody exchanged are of no further use.
    db.execute("DELETE FROM authorization_codes WHERE expires_at <= ?1", [now])?;
    db.execute(
        "INSERT INTO authorization_codes (digest, client_id, member_id, redirect_uri, code_challenge, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![digest(&code), client.id, member_id, redirect_uri, challenge, now + CODE_LIFETIME],
    )?;
    let redirect_to = with_query(&redirect_uri, &[("code", Some(&code)), ("state", state)]);
    Ok(ConnectionRedirect { redirect_to }.into())
}

/// The PKCE code challenge, if the client sent one Turtle accepts: S256
/// only, since the plain method protects nothing (OAuth 2.1).
pub fn pkce_challenge<'a>(challenge: Option<&'a str>, method: Option<&str>) -> Result<&'a str, &'static str> {
    let Some(challenge) = challenge.filter(|challenge| !challenge.is_empty()) else {
        return Err("A code_challenge is required (PKCE).");
    };
    if method != Some("S256") {
        return Err("Only the S256 code_challenge_method is supported.");
    }
    if !is_pkce_value(challenge) {
        return Err("The code_challenge is not valid.");
    }
    Ok(challenge)
}

/// Whether a PKCE value has the length and characters RFC 7636 allows.
fn is_pkce_value(value: &str) -> bool {
    (43..=128).contains(&value.len())
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~'))
}

/// An address with parameters added to its query, leaving out those
/// without a value.
pub fn with_query(address: &str, parameters: &[(&str, Option<&str>)]) -> String {
    let mut result = address.to_string();
    for (name, value) in parameters {
        let Some(value) = value else { continue };
        result.push(if result.contains('?') { '&' } else { '?' });
        result.push_str(name);
        result.push('=');
        result.push_str(&percent_encode(value));
    }
    result
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// What a client sends to exchange its authorization code.
#[derive(Debug, Default)]
pub struct CodeExchange {
    pub code: String,
    pub client_id: String,
    pub redirect_uri: Option<String>,
    pub code_verifier: String,
}

/// An access token for a connection.
#[derive(Debug, PartialEq, Eq)]
pub struct IssuedAccess {
    pub access_token: String,
    /// In seconds.
    pub expires_in: i64,
}

/// Why a code was not exchanged, in OAuth's terms.
#[derive(Debug)]
pub enum GrantRefusal {
    /// The code is unknown, used, expired, for another client or redirect
    /// address, or the code verifier does not match it.
    InvalidGrant,
    Failed(OpError),
}

impl From<OpError> for GrantRefusal {
    fn from(error: OpError) -> Self {
        GrantRefusal::Failed(error)
    }
}

impl From<rusqlite::Error> for GrantRefusal {
    fn from(error: rusqlite::Error) -> Self {
        GrantRefusal::Failed(error.into())
    }
}

/// Exchanges an authorization code for the connection it allows: a new
/// Token of the Member, named after the client, and its first access token.
/// A code works once, whether the exchange succeeds or not.
pub fn exchange_code(core: &Core, input: CodeExchange) -> Result<IssuedAccess, GrantRefusal> {
    require_oauth(core)?;
    let now = core.now();
    let mut db = core.db();
    let transaction = db.transaction()?;
    let found = transaction
        .query_row(
            "DELETE FROM authorization_codes WHERE digest = ?1
             RETURNING client_id, member_id, redirect_uri, code_challenge, expires_at",
            [digest(&input.code)],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()
        ?;
    // The code is gone even if what follows refuses it.
    transaction.commit()?;
    let Some((client_id, member_id, redirect_uri, challenge, expires_at)) = found else {
        return Err(GrantRefusal::InvalidGrant);
    };
    let redirect_matches = input.redirect_uri.as_ref().is_none_or(|uri| *uri == redirect_uri);
    if expires_at <= now || client_id != input.client_id || !redirect_matches || !verifier_matches(&input.code_verifier, &challenge) {
        return Err(GrantRefusal::InvalidGrant);
    }
    let Some(client) = client(&db, &client_id)? else { return Err(GrantRefusal::InvalidGrant) };
    let active: bool = db
        .query_row("SELECT deactivated_at IS NULL FROM members WHERE id = ?1", [&member_id], |row| row.get(0))
        .optional()
        ?
        .unwrap_or(false);
    if !active {
        return Err(GrantRefusal::InvalidGrant);
    }

    let token_id = random_hex(16)?;
    // The connection's own secret is never handed out; the client holds
    // only access tokens for it.
    let unused_secret = random_hex(32)?;
    let access_token = format!("{ACCESS_TOKEN_PREFIX}{}", random_hex(32)?);
    let transaction = db.transaction()?;
    transaction
        .execute(
            "INSERT INTO tokens (id, member_id, name, secret_digest, read_only, all_areas, client_id, created_at)
             VALUES (?1, ?2, ?3, ?4, 0, 1, ?5, ?6)",
            rusqlite::params![token_id, member_id, client.name, digest(&unused_secret), client.id, now],
        )
        ?;
    transaction
        .execute(
            "INSERT INTO access_tokens (digest, token_id, expires_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![digest(&access_token), token_id, now + ACCESS_TOKEN_LIFETIME],
        )
        ?;
    transaction.commit()?;
    Ok(IssuedAccess { access_token, expires_in: ACCESS_TOKEN_LIFETIME })
}

/// Whether a PKCE code verifier is the one its S256 challenge was made from.
fn verifier_matches(verifier: &str, challenge: &str) -> bool {
    if !is_pkce_value(verifier) {
        return false;
    }
    let made = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    made == challenge
}

#[cfg(test)]
mod tests {
    use super::super::testing::{fresh, member, TestCore};
    use super::super::tokens::{author_of, principal_for_token};
    use super::*;

    const CALLBACK: &str = "https://claude.ai/api/mcp/auth_callback";
    /// The worked example of RFC 7636, appendix B.
    const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

    fn offering_oauth() -> TestCore {
        let mut core = fresh();
        core.core.set_public_url(Some("https://turtle.example.org".into()));
        core
    }

    fn claude(core: &Core) -> Client {
        register_client(core, NewClient { redirect_uris: vec![CALLBACK.into()], client_name: Some("Claude".into()) }).unwrap()
    }

    fn answer(client: &Client, allow: bool) -> ConnectionAnswer {
        ConnectionAnswer {
            client_id: client.id.clone(),
            redirect_uri: Some(CALLBACK.into()),
            code_challenge: Some(CHALLENGE.into()),
            code_challenge_method: Some("S256".into()),
            state: Some("a b&c".into()),
            allow,
        }
    }

    /// The value of a parameter of the address the browser is sent to.
    fn parameter(address: &str, name: &str) -> Option<String> {
        let query = address.split_once('?')?.1;
        query.split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            (key == name).then(|| value.to_string())
        })
    }

    fn exchange(client: &Client, code: &str, verifier: &str) -> CodeExchange {
        CodeExchange {
            code: code.into(),
            client_id: client.id.clone(),
            redirect_uri: Some(CALLBACK.into()),
            code_verifier: verifier.into(),
        }
    }

    #[test]
    fn a_member_allows_a_client_which_then_acts_as_them_for_an_hour() {
        let core = offering_oauth();
        let ada = member(&core, "Ada", false);
        let client = claude(&core);

        let request = describe_connection_request(
            &core,
            &ada,
            RequestedConnection { client_id: client.id.clone(), redirect_uri: Some(CALLBACK.into()) },
        )
        .unwrap()
        .output;
        assert_eq!(request, ConnectionRequest { app_name: "Claude".into(), redirect_domain: "claude.ai".into() });

        let redirect = answer_connection_request(&core, &ada, answer(&client, true)).unwrap().output.redirect_to;
        assert!(redirect.starts_with("https://claude.ai/api/mcp/auth_callback?code="), "{redirect}");
        assert_eq!(parameter(&redirect, "state").as_deref(), Some("a%20b%26c"));
        let code = parameter(&redirect, "code").unwrap();

        let issued = exchange_code(&core, exchange(&client, &code, VERIFIER)).unwrap();
        assert_eq!(issued.expires_in, 3600);
        let acting = principal_for_token(&core, &issued.access_token).unwrap().expect("the access token works");
        let Principal::Member { id, token: Some(limits), .. } = &acting else { panic!("{acting:?}") };
        assert_eq!(id, "member-ada");
        assert!(!limits.read_only && limits.areas.is_none(), "full access: {limits:?}");
        // Changes are credited to Ada through the app.
        assert_eq!(
            author_of(&core, &acting).unwrap(),
            crate::live::Author::AgentForMember { member_id: "member-ada".into(), agent: "Claude".into() }
        );

        // A code works once.
        assert!(matches!(exchange_code(&core, exchange(&client, &code, VERIFIER)), Err(GrantRefusal::InvalidGrant)));

        core.advance(3599);
        assert!(principal_for_token(&core, &issued.access_token).unwrap().is_some());
        core.advance(1);
        assert_eq!(principal_for_token(&core, &issued.access_token).unwrap(), None);
    }

    #[test]
    fn a_code_needs_the_matching_code_verifier() {
        let core = offering_oauth();
        let ada = member(&core, "Ada", false);
        let client = claude(&core);
        let other_verifier = "x".repeat(43);
        for verifier in [other_verifier.as_str(), ""] {
            let redirect = answer_connection_request(&core, &ada, answer(&client, true)).unwrap().output.redirect_to;
            let code = parameter(&redirect, "code").unwrap();
            assert!(
                matches!(exchange_code(&core, exchange(&client, &code, verifier)), Err(GrantRefusal::InvalidGrant)),
                "{verifier:?}"
            );
            // Refused once, the code is gone, even with the right verifier.
            assert!(matches!(exchange_code(&core, exchange(&client, &code, VERIFIER)), Err(GrantRefusal::InvalidGrant)));
        }
    }

    #[test]
    fn a_code_is_only_for_its_client_and_only_for_a_few_minutes() {
        let core = offering_oauth();
        let ada = member(&core, "Ada", false);
        let client = claude(&core);
        let other = claude(&core);

        let redirect = answer_connection_request(&core, &ada, answer(&client, true)).unwrap().output.redirect_to;
        let code = parameter(&redirect, "code").unwrap();
        assert!(matches!(exchange_code(&core, exchange(&other, &code, VERIFIER)), Err(GrantRefusal::InvalidGrant)));

        let redirect = answer_connection_request(&core, &ada, answer(&client, true)).unwrap().output.redirect_to;
        let code = parameter(&redirect, "code").unwrap();
        core.advance(5 * 60);
        assert!(matches!(exchange_code(&core, exchange(&client, &code, VERIFIER)), Err(GrantRefusal::InvalidGrant)));
    }

    #[test]
    fn denying_sends_the_client_access_denied() {
        let core = offering_oauth();
        let ada = member(&core, "Ada", false);
        let client = claude(&core);
        let redirect = answer_connection_request(&core, &ada, answer(&client, false)).unwrap().output.redirect_to;
        assert_eq!(redirect, "https://claude.ai/api/mcp/auth_callback?error=access_denied&state=a%20b%26c");
    }

    #[test]
    fn a_client_without_s256_pkce_gets_an_error_and_no_code() {
        let core = offering_oauth();
        let ada = member(&core, "Ada", false);
        let client = claude(&core);
        for (challenge, method) in [(None, None), (Some(CHALLENGE), Some("plain")), (Some(CHALLENGE), None)] {
            let mut asked = answer(&client, true);
            asked.code_challenge = challenge.map(Into::into);
            asked.code_challenge_method = method.map(Into::into);
            let redirect = answer_connection_request(&core, &ada, asked).unwrap().output.redirect_to;
            assert_eq!(parameter(&redirect, "error").as_deref(), Some("invalid_request"), "{redirect}");
            assert_eq!(parameter(&redirect, "code"), None);
        }
    }

    #[test]
    fn only_a_member_signed_in_in_a_browser_decides_on_a_connection() {
        let core = offering_oauth();
        let ada = member(&core, "Ada", false);
        let client = claude(&core);
        let redirect = answer_connection_request(&core, &ada, answer(&client, true)).unwrap().output.redirect_to;
        let code = parameter(&redirect, "code").unwrap();
        let issued = exchange_code(&core, exchange(&client, &code, VERIFIER)).unwrap();
        let through_token = principal_for_token(&core, &issued.access_token).unwrap().unwrap();

        let agent = Principal::Agent { id: "agent".into(), token: super::super::TokenLimits { id: "t".into(), read_only: false, areas: None } };
        for refused in [Principal::Anonymous, agent, through_token] {
            let error = answer_connection_request(&core, &refused, answer(&client, true)).unwrap_err();
            assert_eq!(error.kind(), "not_permitted", "{refused:?}");
        }
    }

    #[test]
    fn a_request_names_a_registered_client_and_one_of_its_redirect_addresses() {
        let core = offering_oauth();
        let ada = member(&core, "Ada", false);
        let client = claude(&core);
        for (client_id, redirect_uri) in [
            ("made-up", Some(CALLBACK)),
            (client.id.as_str(), Some("https://evil.example.com/callback")),
        ] {
            let request = RequestedConnection { client_id: client_id.into(), redirect_uri: redirect_uri.map(Into::into) };
            let error = describe_connection_request(&core, &ada, request).unwrap_err();
            assert_eq!(error.kind(), "not_found");
        }
        // A client with one redirect address need not repeat it.
        let request = RequestedConnection { client_id: client.id.clone(), redirect_uri: None };
        assert_eq!(describe_connection_request(&core, &ada, request).unwrap().output.redirect_domain, "claude.ai");
    }

    #[test]
    fn clients_register_with_redirect_addresses_that_reach_only_them() {
        let core = offering_oauth();
        for uri in ["https://claude.ai/api/mcp/auth_callback", "http://localhost:33418/callback", "http://127.0.0.1:5000/cb"] {
            let registered = register_client(&core, NewClient { redirect_uris: vec![uri.into()], client_name: None });
            assert!(registered.is_ok(), "{uri}");
        }
        for uri in ["http://claude.ai/callback", "javascript:alert(1)", "https://claude.ai/cb#here", "https:///cb", "not an address"] {
            let registered = register_client(&core, NewClient { redirect_uris: vec![uri.into()], client_name: None });
            assert_eq!(registered.unwrap_err().kind(), "invalid_input", "{uri}");
        }
        assert_eq!(register_client(&core, NewClient::default()).unwrap_err().kind(), "invalid_input");
        // Without a name of its own, a client is known by its domain.
        let nameless = register_client(&core, NewClient { redirect_uris: vec!["https://app.example.com:8443/cb".into()], client_name: None });
        assert_eq!(nameless.unwrap().name, "app.example.com");
    }

    #[test]
    fn without_a_public_url_nothing_of_oauth_exists() {
        let core = fresh();
        let ada = member(&core, "Ada", false);
        let registered = register_client(&core, NewClient { redirect_uris: vec![CALLBACK.into()], client_name: None });
        assert_eq!(registered.unwrap_err().kind(), "not_found");
        let request = RequestedConnection { client_id: "any".into(), redirect_uri: Some(CALLBACK.into()) };
        assert_eq!(describe_connection_request(&core, &ada, request).unwrap_err().kind(), "not_found");
    }

    #[test]
    fn deactivating_a_member_ends_their_connections() {
        let core = offering_oauth();
        let admin = member(&core, "Grace", true);
        let ada = member(&core, "Ada", false);
        let client = claude(&core);
        let redirect = answer_connection_request(&core, &ada, answer(&client, true)).unwrap().output.redirect_to;
        let code = parameter(&redirect, "code").unwrap();
        let issued = exchange_code(&core, exchange(&client, &code, VERIFIER)).unwrap();

        super::super::members::deactivate_member(&core, &admin, super::super::members::MemberId { id: "member-ada".into() }).unwrap();
        assert_eq!(principal_for_token(&core, &issued.access_token).unwrap(), None);
    }
}
