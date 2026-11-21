//! Turtle server: one binary with the web app inside and one data directory.

mod http_api;
mod markdown;
mod model;
mod ops;
mod store;
mod web_app;
mod ydoc;

use anyhow::Context;
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use yrs::updates::decoder::Decode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

// The first byte of every WebSocket frame says what the rest is.
/// A document update, in both directions.
const FRAME_UPDATE: u8 = 0;
/// Presence (who is here, where their cursor is). Relayed, never interpreted.
const FRAME_PRESENCE: u8 = 1;
/// Sent by the server when someone joins, asking the others to announce themselves.
const FRAME_PRESENCE_REQUEST: u8 = 2;

/// How often changed Pages are written to the database (ADR-0004: never per keystroke).
const SAVE_INTERVAL: Duration = Duration::from_secs(2);

/// Where the data directory is when `TURTLE_DATA` does not say: beside
/// wherever Turtle was started.
const DEFAULT_DATA_DIR: &str = "turtle-data";

/// A Page that is open in memory.
struct OpenPage {
    doc: Doc,
    /// Frames for everyone connected, tagged with the connection that sent them.
    frames: broadcast::Sender<(u64, Vec<u8>)>,
    unsaved: AtomicBool,
}

struct App {
    db: Mutex<rusqlite::Connection>,
    pages: Mutex<HashMap<String, Arc<OpenPage>>>,
    next_connection: AtomicU64,
}

impl App {
    fn open_page(&self, id: &str) -> anyhow::Result<Arc<OpenPage>> {
        let mut pages = self.pages.lock().unwrap();
        if let Some(page) = pages.get(id) {
            return Ok(page.clone());
        }
        let doc = Doc::new();
        let stored: Option<Vec<u8>> = {
            let db = self.db.lock().unwrap();
            let mut statement = db.prepare("SELECT state FROM pages WHERE id = ?1")?;
            let mut rows = statement.query([id])?;
            match rows.next()? {
                Some(row) => Some(row.get(0)?),
                None => None,
            }
        };
        if let Some(state) = stored {
            let update = Update::decode_v1(&state).context("stored page is unreadable")?;
            doc.transact_mut().apply_update(update)?;
        }
        let (frames, _) = broadcast::channel(1024);
        let page = Arc::new(OpenPage {
            doc,
            frames,
            unsaved: AtomicBool::new(false),
        });
        pages.insert(id.to_string(), page.clone());
        Ok(page)
    }

    fn save_changed_pages(&self) -> anyhow::Result<()> {
        let open: Vec<(String, Arc<OpenPage>)> = self
            .pages
            .lock()
            .unwrap()
            .iter()
            .map(|(id, page)| (id.clone(), page.clone()))
            .collect();
        for (id, page) in open {
            if !page.unsaved.swap(false, Ordering::SeqCst) {
                continue;
            }
            let state = page
                .doc
                .transact()
                .encode_state_as_update_v1(&StateVector::default());
            let saved = self.db.lock().unwrap().execute(
                "INSERT INTO pages (id, state) VALUES (?1, ?2)
                 ON CONFLICT (id) DO UPDATE SET state = excluded.state",
                rusqlite::params![id, state],
            );
            if let Err(error) = saved {
                // Keep the page marked so the next round tries again.
                page.unsaved.store(true, Ordering::SeqCst);
                return Err(error.into());
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // `turtle catalogue` prints every operation with its schemas; the web
    // app's types are generated from it.
    if std::env::args().nth(1).as_deref() == Some("catalogue") {
        println!("{:#}", ops::catalogue().describe());
        return Ok(());
    }

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

    // Opening the core brings the schema up to date; the Page store gets its
    // own connection so live editing does not wait behind operations.
    let db_path = store::database_in(&data_dir)?;
    let core = ops::Core::open(&db_path)?;
    let db = store::open(&db_path)?;

    let app = Arc::new(App {
        db: Mutex::new(db),
        pages: Mutex::new(HashMap::new()),
        next_connection: AtomicU64::new(1),
    });

    let saver = app.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(SAVE_INTERVAL);
        loop {
            tick.tick().await;
            let app = saver.clone();
            match tokio::task::spawn_blocking(move || app.save_changed_pages()).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("saving pages failed: {error:#}"),
                Err(error) => eprintln!("saving pages panicked: {error}"),
            }
        }
    });

    let router = Router::new()
        .route("/ws/{page}", get(connect))
        .route(
            "/api/pages/{page}/markdown",
            get(read_markdown).put(write_markdown),
        )
        .with_state(app.clone())
        .merge(http_api::router(core))
        .merge(web_app::router(web_app::EMBEDDED));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    println!(
        "Turtle is listening on http://127.0.0.1:{port} (data directory: {})",
        data_dir.display()
    );
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;

    app.save_changed_pages()?;
    Ok(())
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

async fn read_markdown(
    State(app): State<Arc<App>>,
    Path(page_id): Path<String>,
) -> Result<Response, Failure> {
    let page = app.open_page(&page_id)?;
    let markdown = markdown::to_markdown(&ydoc::read(&page.doc));
    Ok(([(header::CONTENT_TYPE, "text/markdown; charset=utf-8")], markdown).into_response())
}

/// Replaces the Page with the given Markdown. Connected editors see the change at once.
async fn write_markdown(
    State(app): State<Arc<App>>,
    Path(page_id): Path<String>,
    body: String,
) -> Result<StatusCode, Failure> {
    let page = app.open_page(&page_id)?;
    let update = ydoc::write(&page.doc, &markdown::from_markdown(&body));
    page.unsaved.store(true, Ordering::SeqCst);
    let _ = page.frames.send((0, frame(FRAME_UPDATE, &update)));
    Ok(StatusCode::NO_CONTENT)
}

async fn connect(
    State(app): State<Arc<App>>,
    Path(page_id): Path<String>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, Failure> {
    let page = app.open_page(&page_id)?;
    let connection = app.next_connection.fetch_add(1, Ordering::SeqCst);
    Ok(upgrade.on_upgrade(move |socket| async move {
        if let Err(error) = serve_connection(socket, page, connection).await {
            eprintln!("connection {connection} ended with an error: {error:#}");
        }
    }))
}

fn frame(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 1);
    frame.push(kind);
    frame.extend_from_slice(payload);
    frame
}

async fn serve_connection(
    socket: WebSocket,
    page: Arc<OpenPage>,
    connection: u64,
) -> anyhow::Result<()> {
    let (mut outgoing, mut incoming) = socket.split();

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
    let _ = page
        .frames
        .send((connection, vec![FRAME_PRESENCE_REQUEST]));

    let mut own_presence: Option<(u64, u64)> = None;
    let result = loop {
        tokio::select! {
            message = incoming.next() => {
                let data = match message {
                    Some(Ok(Message::Binary(data))) => data,
                    Some(Ok(Message::Close(_))) | None => break Ok(()),
                    Some(Ok(_)) => continue,
                    Some(Err(error)) => break Err(error.into()),
                };
                match data.first() {
                    Some(&FRAME_UPDATE) => {
                        let applied = Update::decode_v1(&data[1..])
                            .context("unreadable update")
                            .and_then(|update| Ok(page.doc.transact_mut().apply_update(update)?));
                        if let Err(error) = applied {
                            break Err(error);
                        }
                        page.unsaved.store(true, Ordering::SeqCst);
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

fn read_varuint(data: &mut &[u8]) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let (&byte, rest) = data.split_first()?;
        *data = rest;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn write_varuint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

#[cfg(test)]
mod tests {
    use super::*;

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
