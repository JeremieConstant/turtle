//! Turtle server: one binary with the web app inside and one data directory.

mod demo;
mod diff;
mod directive;
mod http_api;
mod live;
mod mail;
mod markdown;
mod mcp;
mod model;
mod oauth;
mod ops;
mod pdf;
mod public_url;
mod release;
mod search;
mod store;
mod web_app;
mod ydoc;

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use live::{frame, read_varuint, write_varuint, OpenPage, FRAME_PRESENCE, FRAME_PRESENCE_REQUEST, FRAME_TAGS, FRAME_UPDATE};
use ops::permissions::Role;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use yrs::{ReadTxn, StateVector, Transact};

/// How often changed Pages are written to the database (ADR-0004: never per
/// keystroke), and Versions that are due recorded.
const SAVE_INTERVAL: Duration = Duration::from_secs(2);

/// How often an open editing connection checks that its session is still
/// valid and the Page still readable, even when nothing is sent. Changes are
/// checked each time they arrive, and a session that is ended or a Grant that
/// changes is noticed at once; this catches what nobody announces, such as a
/// session expiring.
const ACCESS_RECHECK_INTERVAL: Duration = Duration::from_secs(15);

/// How often the search index takes in what changed. With `SAVE_INTERVAL`
/// this bounds how soon an edit can be found.
const SEARCH_INTERVAL: Duration = Duration::from_secs(1);
/// How often what has been in the Trash for too long is removed.
const TRASH_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Where the data directory is when `TURTLE_DATA` does not say: beside
/// wherever Turtle was started.
const DEFAULT_DATA_DIR: &str = "turtle-data";

struct App {
    /// Decides who may open which Page, and holds the open Pages.
    core: Arc<ops::Core>,
    next_connection: AtomicU64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // `turtle catalogue` prints every operation with its schemas; the web
    // app's types are generated from it.
    match std::env::args().nth(1).as_deref() {
        Some("catalogue") => {
            println!("{:#}", ops::catalogue().describe());
            return Ok(());
        }
        // Says which Release this is, without opening the data directory.
        Some("--version") => {
            println!("turtle {}", release::RELEASE.number);
            return Ok(());
        }
        _ => {}
    }
    println!("Starting Turtle {}.", release::RELEASE.number);

    // Everything Turtle keeps lives in one directory.
    let data_dir = PathBuf::from(
        std::env::var_os("TURTLE_DATA")
            .filter(|dir| !dir.is_empty())
            .unwrap_or_else(|| DEFAULT_DATA_DIR.into()),
    );
    let port: u16 = std::env::var("TURTLE_PORT")
        .or_else(|_| std::env::var("PORT"))
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(4100);
    // Loopback unless the operator says otherwise: a container has to listen
    // on every address to be reached from outside it.
    let host: IpAddr = match std::env::var("TURTLE_HOST") {
        Ok(host) if !host.trim().is_empty() => match host.trim().parse() {
            Ok(address) => address,
            Err(_) => anyhow::bail!(
                "TURTLE_HOST must be an IP address to listen on, such as 127.0.0.1 or 0.0.0.0, not {host:?}"
            ),
        },
        _ => Ipv4Addr::LOCALHOST.into(),
    };

    // The largest file that can be attached, and the largest ZIP that can be imported.
    let attachment_limit = megabytes("TURTLE_MAX_ATTACHMENT_MB", ops::attachments::DEFAULT_ATTACHMENT_LIMIT)?;
    let import_limit = megabytes("TURTLE_MAX_IMPORT_MB", ops::import::DEFAULT_IMPORT_LIMIT)?;

    // Where people and AI clients reach this Turtle from outside, if it is
    // known; only then is OAuth offered (ADR-0010).
    let public_url = match std::env::var("TURTLE_PUBLIC_URL") {
        Ok(url) if !url.trim().is_empty() => Some(public_url::PublicUrl::parse(&url)?),
        _ => None,
    };

    // Invitations and password reset links are mailed when an SMTP server
    // is set up; otherwise they are passed on by hand.
    let smtp_url = std::env::var("TURTLE_SMTP_URL").ok();
    let mail_from = std::env::var("TURTLE_MAIL_FROM").ok();
    let mailer = mail::from_settings(smtp_url.as_deref(), mail_from.as_deref(), public_url.as_ref())?;

    // A Demo says so on every screen and shows its demo Member on the sign-in
    // screen; the hosting resets it.
    let demo_email = std::env::var("TURTLE_DEMO_EMAIL").ok();
    let demo_password = std::env::var("TURTLE_DEMO_PASSWORD").ok();
    let demo_reset_hours = std::env::var("TURTLE_DEMO_RESET_HOURS").ok();
    let demo = demo::from_settings(demo_email.as_deref(), demo_password.as_deref(), demo_reset_hours.as_deref())?;

    // Behind a reverse proxy every request comes from the proxy, so failed
    // attempts are counted by the client address the proxy passes on in
    // this header. Without it the header is not trusted, as anyone can send it.
    let client_address_header = match std::env::var("TURTLE_CLIENT_ADDRESS_HEADER") {
        Ok(name) if !name.trim().is_empty() => Some(name.trim().parse::<header::HeaderName>().map_err(|_| {
            anyhow::anyhow!("TURTLE_CLIENT_ADDRESS_HEADER must be the name of an HTTP header, such as X-Forwarded-For, not {name:?}")
        })?),
        _ => None,
    };

    // Opening the core brings the schema up to date.
    let db_path = store::database_in(&data_dir)?;
    let mut core = ops::Core::open(&db_path)?;
    core.set_attachment_limit(attachment_limit);
    core.set_import_limit(import_limit);
    core.set_public_url(public_url);
    core.set_mailer(mailer.map(|smtp| Arc::new(smtp) as Arc<dyn mail::Mailer>));
    core.set_demo(demo);
    let core = core.shared();
    // Whether Turtle looks reachable only from its intranet may take a name
    // lookup; it is found out now, off the async workers, so the Connect AI
    // page need not wait for it.
    tokio::task::spawn_blocking({
        let core = core.clone();
        move || core.looks_intranet_only()
    });

    let app = Arc::new(App {
        core: core.clone(),
        next_connection: AtomicU64::new(1),
    });

    // For load tests and for an operator who wants to see whether saving
    // keeps up: one line per save that wrote anything.
    let log_saves = std::env::var_os("TURTLE_LOG_SAVES").is_some_and(|value| !value.is_empty());
    let saver = core.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(SAVE_INTERVAL);
        loop {
            tick.tick().await;
            let core = saver.clone();
            let started = std::time::Instant::now();
            match tokio::task::spawn_blocking(move || core.save_changed_pages()).await {
                Ok(Ok(0)) => {}
                Ok(Ok(pages)) => {
                    if log_saves {
                        let took = started.elapsed().as_secs_f64() * 1000.0;
                        let noun = if pages == 1 { "Page" } else { "Pages" };
                        println!("Saved {pages} {noun} in {took:.3} ms.");
                    }
                }
                Ok(Err(error)) => eprintln!("saving pages failed: {error:#}"),
                Err(error) => eprintln!("saving pages panicked: {error}"),
            }
        }
    });

    // The first round rebuilds the index if it was missing.
    let indexer = core.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(SEARCH_INTERVAL);
        loop {
            tick.tick().await;
            let core = indexer.clone();
            match tokio::task::spawn_blocking(move || core.catch_up_search()).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("updating the search index failed: {error:#}"),
                Err(error) => eprintln!("updating the search index panicked: {error}"),
            }
        }
    });

    // What has been in the Trash long enough goes once a day, first at start.
    let cleaner = core.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(TRASH_INTERVAL);
        loop {
            tick.tick().await;
            let core = cleaner.clone();
            match tokio::task::spawn_blocking(move || ops::trash::remove_expired(&core)).await {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => eprintln!("emptying the Trash failed: {error:#}"),
                Err(error) => eprintln!("emptying the Trash panicked: {error}"),
            }
        }
    });

    let router = Router::new()
        .route("/ws/{page}", get(connect))
        .route(
            "/api/pages/{page}/markdown",
            get(read_markdown).put(write_markdown),
        )
        .route("/api/attachments/{id}", get(download_attachment))
        .route("/api/pdf_style_logos/{id}", get(show_pdf_style_logo))
        .route("/api/profile_pictures/{id}", get(show_profile_picture))
        .with_state(app.clone())
        .merge(http_api::router(core.clone(), client_address_header))
        .merge(oauth::router(core.clone()))
        .merge(mcp::router(core))
        .merge(web_app::router(web_app::EMBEDDED));

    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    // With port 0 the system picks one; the line says which (the tests read it).
    let address = listener.local_addr()?;
    println!(
        "Turtle is listening on http://{address} (data directory: {})",
        data_dir.display()
    );
    axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(shutdown_requested())
        .await?;

    app.core.save_before_stopping()?;
    app.core.close_search()?;
    Ok(())
}

/// Ctrl+C, or SIGTERM as sent by service managers, container runtimes and `kill`.
async fn shutdown_requested() {
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                terminate.recv().await;
            }
            Err(error) => {
                eprintln!("cannot listen for SIGTERM, only Ctrl+C stops Turtle: {error}");
                std::future::pending().await
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        () = terminate => {}
    }
}

struct Failure(anyhow::Error);

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        eprintln!("request failed: {:#}", self.0);
        (StatusCode::INTERNAL_SERVER_ERROR, format!("{:#}", self.0)).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for Failure {
    fn from(error: E) -> Self {
        Failure(error.into())
    }
}

/// What a request may do with a Page.
#[derive(Clone, Copy)]
enum Use {
    Read,
    Change,
}

/// The Page, if the caller of this request may use it that way. The
/// operation layer decides; this only finds the caller and answers like the
/// API does, except that nobody signed in gets 401. Anything but a Page the
/// caller can read (a Folder, a Page of the first slice without an Area) is
/// not found; a change without the Write Role is not permitted.
// The refusal is the handlers' own response type; boxing it would only move
// the allocation.
#[allow(clippy::result_large_err)]
async fn page_for(
    core: &Arc<ops::Core>,
    headers: HeaderMap,
    page_id: &str,
    using: Use,
) -> Result<Opened, Response> {
    let page_id = page_id.to_string();
    as_caller(core, headers, move |core, principal| {
        let page = match using {
            Use::Read => ops::tree::get_page(core, principal, ops::tree::PageId { id: page_id })?.output,
            Use::Change => ops::tree::page_to_change(core, principal, &page_id)?,
        };
        let author = ops::tokens::author_of(core, principal)?;
        Ok(Opened { page, author })
    })
    .await
}

/// Does `act` as the caller of this request, off the async workers, and
/// answers a refusal like the API does, except that nobody signed in gets 401.
#[allow(clippy::result_large_err)]
async fn as_caller<T: Send + 'static>(
    core: &Arc<ops::Core>,
    headers: HeaderMap,
    act: impl FnOnce(&ops::Core, &ops::Principal) -> Result<T, ops::OpError> + Send + 'static,
) -> Result<T, Response> {
    let core = core.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let principal = http_api::principal(&core, &headers)?;
        let done = act(&core, &principal);
        Ok::<_, http_api::Refusal>(done.map_err(|error| (principal == ops::Principal::Anonymous, error)))
    })
    .await;
    match outcome {
        Ok(Ok(Ok(opened))) => Ok(opened),
        Ok(Ok(Err((true, ops::OpError::NotPermitted(message))))) => {
            Err((StatusCode::UNAUTHORIZED, message).into_response())
        }
        Ok(Ok(Err((_, error)))) => Err(http_api::error_response(error)),
        Ok(Err(no_caller)) => Err(no_caller.into_response()),
        Err(error) => Err(http_api::error_response(ops::OpError::Internal(error.into()))),
    }
}

/// A Page someone may use, and who they are.
struct Opened {
    page: ops::tree::Page,
    author: live::Author,
}

/// The Page as plain Markdown, as `get_page_markdown` gives it.
async fn read_markdown(State(app): State<Arc<App>>, Path(page_id): Path<String>, headers: HeaderMap) -> Response {
    let read = as_caller(&app.core, headers, move |core, principal| {
        ops::content::get_page_markdown(core, principal, ops::content::OfPage { page_id })
    })
    .await;
    match read {
        Ok(reply) => ([(header::CONTENT_TYPE, "text/markdown; charset=utf-8")], reply.output.markdown).into_response(),
        Err(refused) => refused,
    }
}

/// The file of an Attachment, as `read_attachment` gives it. Only images
/// are shown in the browser; every other file is downloaded, and nothing in
/// it runs as part of Turtle.
async fn download_attachment(State(app): State<Arc<App>>, Path(id): Path<String>, headers: HeaderMap) -> Response {
    let opened = as_caller(&app.core, headers, move |core, principal| {
        ops::attachments::open_attachment(core, principal, &id)
    })
    .await;
    let (attachment, content) = match opened {
        Ok(opened) => opened,
        Err(refused) => return refused,
    };
    let shown = if attachment.shown_as_image { "inline" } else { "attachment" };
    let disposition = format!("{shown}; filename*=UTF-8''{}", percent_encoded(&attachment.name));
    (
        [
            (header::CONTENT_TYPE, attachment.content_type),
            (header::CONTENT_DISPOSITION, disposition),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
            (header::CONTENT_SECURITY_POLICY, "sandbox".into()),
            // Someone who loses access must not find it in a shared cache.
            (header::CACHE_CONTROL, "private".into()),
        ],
        content,
    )
        .into_response()
}

/// A PDF Style's logo, as `open_pdf_style_logo` gives it to a Workspace
/// Admin, for the settings to show in an `<img>`. Opened on its own it is
/// downloaded, and sandboxed besides: an SVG could carry scripts.
async fn show_pdf_style_logo(State(app): State<Arc<App>>, Path(id): Path<String>, headers: HeaderMap) -> Response {
    let opened = as_caller(&app.core, headers, move |core, principal| {
        ops::pdf_styles::open_pdf_style_logo(core, principal, &id)
    })
    .await;
    let logo = match opened {
        Ok(logo) => logo,
        Err(refused) => return refused,
    };
    (
        [
            (header::CONTENT_TYPE, logo.format.content_type().to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename*=UTF-8''{}", percent_encoded(&logo.file_name))),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
            (header::CONTENT_SECURITY_POLICY, "sandbox".into()),
            (header::CACHE_CONTROL, "private".into()),
        ],
        logo.content,
    )
        .into_response()
}

/// A Profile picture, as `open_profile_picture` gives it to whoever sees the
/// Profile, for an `<img>`. Turtle made the PNG itself, so it is shown.
async fn show_profile_picture(State(app): State<Arc<App>>, Path(id): Path<String>, headers: HeaderMap) -> Response {
    let opened = as_caller(&app.core, headers, move |core, principal| {
        ops::profiles::open_profile_picture(core, principal, &id)
    })
    .await;
    match opened {
        Ok(png) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                // Someone who loses sight of the Profile must not find it in a shared cache.
                (header::CACHE_CONTROL, "private"),
            ],
            png,
        )
            .into_response(),
        Err(refused) => refused,
    }
}

/// A file name as a header may carry it (RFC 8187): every byte but letters,
/// digits and `-._~` as %XX.
fn percent_encoded(name: &str) -> String {
    name.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// Makes the Page say what the given Markdown says, as `replace_page` does.
async fn write_markdown(
    State(app): State<Arc<App>>,
    Path(page_id): Path<String>,
    headers: HeaderMap,
    markdown: String,
) -> Response {
    let written = as_caller(&app.core, headers, move |core, principal| {
        ops::content::replace_page(core, principal, ops::content::NewContent { page_id, markdown })
    })
    .await;
    match written {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(refused) => refused,
    }
}

async fn connect(
    State(app): State<Arc<App>>,
    Path(page_id): Path<String>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response, Failure> {
    // Watched before access is checked, so a change in between is noticed.
    let access_changes = app.core.watch_access();
    let (role, author) = match page_for(&app.core, headers.clone(), &page_id, Use::Read).await {
        Ok(opened) => (opened.page.role, opened.author),
        Err(refused) => return Ok(refused),
    };
    let page = app.core.live().open(&page_id)?;
    let connection = app.next_connection.fetch_add(1, Ordering::SeqCst);
    let access = Access {
        core: app.core.clone(),
        headers,
        page_id,
        role,
        author,
        changes: access_changes,
    };
    let core = app.core.clone();
    Ok(upgrade.on_upgrade(move |socket| async move {
        core.live().connected(&page);
        match serve_connection(socket, page.clone(), connection, access).await {
            Err(error) if !went_away(&error) => {
                eprintln!("connection {connection} ended with an error: {error:#}");
            }
            _ => {}
        }
        // Recording a Version writes to the database.
        let left = tokio::task::spawn_blocking(move || core.live().disconnected(&page)).await;
        match left {
            Ok(Ok(())) => {}
            Ok(Err(error)) => eprintln!("recording a Version failed: {error:#}"),
            Err(error) => eprintln!("recording a Version panicked: {error}"),
        }
    }))
}

/// What an open editing connection needs to check its access again: the
/// request it was opened with, the Page it is for and the Role it was opened
/// with.
struct Access {
    core: Arc<ops::Core>,
    headers: HeaderMap,
    page_id: String,
    role: Role,
    /// Who is editing, to be named among the authors of the next Version.
    author: live::Author,
    /// Fires whenever someone's access may have changed.
    changes: tokio::sync::watch::Receiver<u64>,
}

impl Access {
    /// Whether the connection still holds the Role it was opened with. Once
    /// it does not, it is closed: the editor opens the Page again and gets
    /// its new Role, or finds it can no longer read it.
    async fn still_holds_role(&self) -> bool {
        match page_for(&self.core, self.headers.clone(), &self.page_id, Use::Read).await {
            Ok(opened) => opened.page.role == self.role,
            Err(_) => false,
        }
    }

    /// Whether the connection may still use the Page that way now. It may
    /// not once the session ended or expired, the Page can no longer be
    /// read, or, for a change, the Write Role is gone.
    async fn allows(&self, using: Use) -> Result<(), StatusCode> {
        match page_for(&self.core, self.headers.clone(), &self.page_id, using).await {
            Ok(_) => Ok(()),
            Err(refused) => Err(refused.status()),
        }
    }
}

async fn serve_connection(
    socket: WebSocket,
    page: Arc<OpenPage>,
    connection: u64,
    mut access: Access,
) -> anyhow::Result<()> {
    let (mut outgoing, mut incoming) = socket.split();

    // A session that ended, or a Grant that changed, since access was checked
    // gets nothing of the Page, not even its content as it is.
    if access.changes.has_changed().unwrap_or(true) {
        access.changes.mark_unchanged();
        if !access.still_holds_role().await {
            return Ok(());
        }
    }

    // Subscribe before reading the state, so no update can fall in between.
    // An update that arrives twice is harmless.
    let mut frames = page.frames.subscribe();
    let state = page
        .doc
        .transact()
        .encode_state_as_update_v1(&StateVector::default());
    outgoing
        .send(Message::Binary(Bytes::from(frame(FRAME_UPDATE, &state))))
        .await?;
    // The Tags as they are now, so a change made before this connection, or
    // while an editor was reconnecting, is not missed. One made meanwhile
    // follows as a frame of its own.
    let tags = {
        let core = access.core.clone();
        let page_id = access.page_id.clone();
        tokio::task::spawn_blocking(move || ops::tags::tags_of(&core.db(), &page_id)).await??
    };
    outgoing
        .send(Message::Binary(Bytes::from(frame(FRAME_TAGS, &serde_json::to_vec(&tags)?))))
        .await?;
    let _ = page
        .frames
        .send((connection, vec![FRAME_PRESENCE_REQUEST]));

    let mut own_presence: Option<(u64, u64)> = None;
    let mut recheck = tokio::time::interval_at(
        tokio::time::Instant::now() + ACCESS_RECHECK_INTERVAL,
        ACCESS_RECHECK_INTERVAL,
    );
    let result = loop {
        tokio::select! {
            // In this order: once a session ended or a Role changed, no frame
            // that arrives meanwhile is passed on before the change is checked.
            biased;
            changed = access.changes.changed() => {
                if changed.is_err() || !access.still_holds_role().await {
                    break Ok(());
                }
            }
            _ = recheck.tick() => {
                if !access.still_holds_role().await {
                    break Ok(());
                }
            }
            message = incoming.next() => {
                let data = match message {
                    Some(Ok(Message::Binary(data))) => data,
                    Some(Ok(Message::Close(_))) | None => break Ok(()),
                    Some(Ok(_)) => continue,
                    Some(Err(error)) => break Err(error.into()),
                };
                match data.first() {
                    Some(&FRAME_UPDATE) => {
                        // Checked for every change, so one made after signing
                        // out or losing the Write Role never lands. Without
                        // Write the change is dropped; the reader stays.
                        match access.allows(Use::Change).await {
                            Ok(()) => {}
                            Err(StatusCode::FORBIDDEN) => continue,
                            Err(_) => break Ok(()),
                        }
                        if let Err(error) = access.core.live().apply(&page, &data[1..], &access.author) {
                            break Err(error);
                        }
                        let _ = page.frames.send((connection, data.to_vec()));
                    }
                    Some(&FRAME_PRESENCE) => {
                        // The first presence a connection sends is its own.
                        for (client, clock) in presence_entries(&data[1..]) {
                            match &mut own_presence {
                                None => own_presence = Some((client, clock)),
                                Some((own, latest)) if *own == client => *latest = clock.max(*latest),
                                Some(_) => {}
                            }
                        }
                        let _ = page.frames.send((connection, data.to_vec()));
                    }
                    _ => {}
                }
            }
            received = frames.recv() => match received {
                Ok((sender, data)) if sender != connection => {
                    if let Err(error) = outgoing.send(Message::Binary(Bytes::from(data))).await {
                        break Err(error.into());
                    }
                }
                Ok(_) => {}
                // This connection fell too far behind. Closing makes the
                // editor reconnect and receive the full state again.
                Err(broadcast::error::RecvError::Lagged(_)) => break Ok(()),
                Err(broadcast::error::RecvError::Closed) => break Ok(()),
            }
        }
    };

    // Tell the others this person is gone, so their cursor disappears at once
    // instead of lingering until it times out.
    if let Some((client, clock)) = own_presence {
        let mut payload = vec![1];
        write_varuint(&mut payload, client);
        write_varuint(&mut payload, clock + 1);
        write_varuint(&mut payload, 4);
        payload.extend_from_slice(b"null");
        let _ = page.frames.send((connection, frame(FRAME_PRESENCE, &payload)));
    }
    result
}

/// Reads the `(client, clock)` pairs from a presence update. The states
/// themselves are skipped; the server never interprets them.
fn presence_entries(mut data: &[u8]) -> Vec<(u64, u64)> {
    let mut entries = Vec::new();
    let Some(count) = read_varuint(&mut data) else {
        return entries;
    };
    for _ in 0..count {
        let (Some(client), Some(clock), Some(len)) = (
            read_varuint(&mut data),
            read_varuint(&mut data),
            read_varuint(&mut data),
        ) else {
            break;
        };
        let Some(rest) = data.get(len as usize..) else {
            break;
        };
        data = rest;
        entries.push((client, clock));
    }
    entries
}

/// Whether a connection ended because the editor dropped it without saying
/// goodbye, as a closed tab or a lost network does. That is how editing
/// sessions usually end, not a failure.
fn went_away(error: &anyhow::Error) -> bool {
    use std::io::ErrorKind;
    use tungstenite::error::ProtocolError;
    error.chain().any(|cause| {
        let dropped = cause.downcast_ref::<std::io::Error>().is_some_and(|io| {
            matches!(io.kind(), ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::BrokenPipe)
        });
        let reset = matches!(
            cause.downcast_ref::<tungstenite::Error>(),
            Some(tungstenite::Error::Protocol(ProtocolError::ResetWithoutClosingHandshake))
        );
        dropped || reset
    })
}



/// A size in bytes from the setting `name`, given in MB, or `default` when it is unset.
fn megabytes(name: &str, default: u64) -> anyhow::Result<u64> {
    match std::env::var(name) {
        Ok(mb) => match mb.trim().parse::<u64>() {
            Ok(mb) if mb > 0 => Ok(mb * 1024 * 1024),
            _ => anyhow::bail!("{name} must be a whole number of MB above 0, not {mb:?}"),
        },
        Err(_) => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_editor_that_drops_the_connection_has_gone_away_but_a_bad_update_has_not() {
        use std::io::{Error, ErrorKind};
        for kind in [ErrorKind::ConnectionReset, ErrorKind::ConnectionAborted, ErrorKind::BrokenPipe] {
            // Wrapped the way axum hands it over.
            let dropped = anyhow::Error::from(axum::Error::new(tungstenite::Error::Io(Error::from(kind))));
            assert!(went_away(&dropped), "{kind:?}");
        }
        let reset = tungstenite::Error::Protocol(tungstenite::error::ProtocolError::ResetWithoutClosingHandshake);
        assert!(went_away(&anyhow::Error::from(axum::Error::new(reset))), "reset without a closing handshake");
        assert!(!went_away(&anyhow::anyhow!("unreadable update")));
        assert!(!went_away(&anyhow::Error::from(Error::from(ErrorKind::PermissionDenied))));
    }

    #[test]
    fn presence_entries_are_read_without_their_states() {
        let mut payload = vec![2];
        for (client, clock, state) in [(300u64, 5u64, "{\"a\":1}"), (7, 129, "null")] {
            write_varuint(&mut payload, client);
            write_varuint(&mut payload, clock);
            write_varuint(&mut payload, state.len() as u64);
            payload.extend_from_slice(state.as_bytes());
        }
        assert_eq!(presence_entries(&payload), vec![(300, 5), (7, 129)]);
        assert_eq!(presence_entries(&[3, 1]), vec![]);
    }
}
