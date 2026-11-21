//! A client that speaks the editor's protocol (web/src/lib/connection.ts):
//! one WebSocket per Page, binary frames whose first byte says what follows.
//! It keeps its own copy of the document, edits it in the editor's layout, and
//! tracks the presence of the others.
//!
//! The protocol is written out here again on purpose, so a change on the
//! server that breaks the editor breaks these tests too.

use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest, Message};
use yrs::types::text::YChange;
use yrs::updates::decoder::Decode;
use yrs::{
    Any, Doc, GetString, Out, ReadTxn, Text, Transact, Update, XmlElementPrelim,
    XmlFragment, XmlOut, XmlTextPrelim,
};

const FRAME_UPDATE: u8 = 0;
const FRAME_PRESENCE: u8 = 1;
const FRAME_PRESENCE_REQUEST: u8 = 2;

/// The fragment the editor binds the document to.
const FRAGMENT: &str = "default";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);

/// What a client sees at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct Content {
    /// The text of each paragraph, in order.
    pub paragraphs: Vec<String>,
    /// The whole document in its XML form, for comparing clients exactly.
    pub document: String,
    /// The presence state (as JSON) of everyone else, by their client id.
    pub others: HashMap<u64, String>,
}

struct Shared {
    doc: Doc,
    /// Presence of the others: client id to (clock, JSON state).
    others: Mutex<HashMap<u64, (u64, String)>>,
    /// This client's own presence: (clock, JSON state).
    own_presence: Mutex<Option<(u64, String)>>,
    /// Told of every update from the server once it is applied.
    observer: Mutex<Option<Observer>>,
}

/// Called with, for each client whose changes an update carries, the clock
/// its last change in it ends at.
type Observer = Box<dyn Fn(&[(u64, u32)]) + Send>;

pub struct TestClient {
    shared: Arc<Shared>,
    outgoing: mpsc::UnboundedSender<Message>,
    reader: JoinHandle<()>,
    writer: JoinHandle<()>,
}

impl TestClient {
    /// Tries to open a Page with the given session cookie (`name=value`) and
    /// returns the HTTP status the server answered the upgrade with.
    pub async fn try_connect(addr: SocketAddr, page: &str, cookie: Option<&str>) -> u16 {
        match tokio::time::timeout(CONNECT_TIMEOUT, open_socket(addr, page, cookie)).await {
            Err(_) => panic!("connecting to {page} timed out"),
            Ok(Ok((_, response))) => response.status().as_u16(),
            Ok(Err(tungstenite::Error::Http(response))) => response.status().as_u16(),
            Ok(Err(error)) => panic!("connecting to {page}: {error}"),
        }
    }

    /// Connects to a Page and returns once the stored content has arrived.
    pub async fn connect(addr: SocketAddr, page: &str, cookie: Option<&str>) -> TestClient {
        let (socket, _) = tokio::time::timeout(CONNECT_TIMEOUT, open_socket(addr, page, cookie))
            .await
            .unwrap_or_else(|_| panic!("connecting to {page} timed out"))
            .unwrap_or_else(|error| panic!("connecting to {page}: {error}"));
        let (mut sink, mut stream) = socket.split();

        let shared = Arc::new(Shared {
            doc: Doc::new(),
            others: Mutex::new(HashMap::new()),
            own_presence: Mutex::new(None),
            observer: Mutex::new(None),
        });
        let (outgoing, mut queue) = mpsc::unbounded_channel::<Message>();

        let writer = tokio::spawn(async move {
            while let Some(message) = queue.recv().await {
                let closing = matches!(message, Message::Close(_));
                if sink.send(message).await.is_err() || closing {
                    break;
                }
            }
        });

        let (loaded, has_loaded) = oneshot::channel();
        let reader = tokio::spawn({
            let shared = shared.clone();
            let outgoing = outgoing.clone();
            async move {
                let mut loaded = Some(loaded);
                while let Some(Ok(message)) = stream.next().await {
                    let Message::Binary(data) = message else { continue };
                    let Some((&kind, payload)) = data.split_first() else { continue };
                    match kind {
                        FRAME_UPDATE => {
                            let update = Update::decode_v1(payload).expect("the server sent an unreadable update");
                            let reaches: Vec<(u64, u32)> = update
                                .insertions(true)
                                .iter()
                                .filter_map(|(client, ranges)| {
                                    Some((client.get(), ranges.iter().map(|range| range.end).max()?))
                                })
                                .collect();
                            shared
                                .doc
                                .transact_mut()
                                .apply_update(update)
                                .expect("the server sent an update that does not apply");
                            if let Some(observe) = shared.observer.lock().unwrap().as_ref() {
                                observe(&reaches);
                            }
                            if let Some(loaded) = loaded.take() {
                                let _ = loaded.send(());
                            }
                        }
                        FRAME_PRESENCE => apply_presence(&shared, payload),
                        FRAME_PRESENCE_REQUEST => {
                            if let Some(frame) = own_presence_frame(&shared) {
                                let _ = outgoing.send(Message::Binary(frame.into()));
                            }
                        }
                        _ => {}
                    }
                }
            }
        });

        tokio::time::timeout(CONNECT_TIMEOUT, has_loaded)
            .await
            .unwrap_or_else(|_| panic!("the content of {page} did not arrive"))
            .unwrap_or_else(|_| panic!("the connection to {page} closed before its content arrived"));

        TestClient {
            shared,
            outgoing,
            reader,
            writer,
        }
    }

    /// This client's id in the document and in presence.
    pub fn client_id(&self) -> u64 {
        self.shared.doc.client_id().get()
    }

    /// The clock this client's latest change ends at; `on_update` tells
    /// another client when an update with it arrives.
    pub fn own_clock(&self) -> u32 {
        self.shared.doc.transact().state_vector().get(&self.shared.doc.client_id())
    }

    /// Calls `observe` for each update from the server, right after it is
    /// applied, with the senders whose changes it carries: (client id, clock
    /// their last change in it ends at), as `own_clock` gives on the sender.
    pub fn on_update(&self, observe: impl Fn(&[(u64, u32)]) + Send + 'static) {
        *self.shared.observer.lock().unwrap() = Some(Box::new(observe));
    }

    /// Adds a paragraph at the end of the Page.
    pub fn append_paragraph(&self, text: &str) {
        let doc = &self.shared.doc;
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let mut txn = doc.transact_mut();
        let paragraph = fragment.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
        paragraph.push_back(&mut txn, XmlTextPrelim::new(text));
        self.send_edit(txn);
    }

    /// Types `text` into a paragraph at a character offset, as if typed in the editor.
    pub fn type_text(&self, paragraph: u32, offset: u32, text: &str) {
        let doc = &self.shared.doc;
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let mut txn = doc.transact_mut();
        let Some(XmlOut::Element(element)) = fragment.get(&txn, paragraph) else {
            panic!("there is no paragraph {paragraph}");
        };
        let Some(XmlOut::Text(target)) = element.get(&txn, 0) else {
            panic!("paragraph {paragraph} holds no text");
        };
        target.insert(&mut txn, offset, text);
        self.send_edit(txn);
    }

    /// Sets this client's presence (a JSON value) and tells the others.
    pub fn set_presence(&self, state: &str) {
        {
            let mut own = self.shared.own_presence.lock().unwrap();
            let clock = own.as_ref().map_or(0, |(clock, _)| clock + 1);
            *own = Some((clock, state.to_string()));
        }
        let frame = own_presence_frame(&self.shared).unwrap();
        let _ = self.outgoing.send(Message::Binary(frame.into()));
    }

    pub fn content(&self) -> Content {
        let doc = &self.shared.doc;
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let txn = doc.transact();
        let paragraphs = fragment
            .children(&txn)
            .filter_map(|node| match node {
                XmlOut::Element(element) if element.tag().as_ref() == "paragraph" => Some(
                    element
                        .children(&txn)
                        .filter_map(|child| match child {
                            XmlOut::Text(text) => Some(plain_text(&txn, &text)),
                            _ => None,
                        })
                        .collect(),
                ),
                _ => None,
            })
            .collect();
        let document = fragment.get_string(&txn);
        drop(txn);
        let others = self
            .shared
            .others
            .lock()
            .unwrap()
            .iter()
            .map(|(&client, (_, state))| (client, state.clone()))
            .collect();
        Content {
            paragraphs,
            document,
            others,
        }
    }

    pub fn paragraphs(&self) -> Vec<String> {
        self.content().paragraphs
    }

    /// Waits until what this client sees satisfies `condition`. Panics with
    /// `what` and the last content seen if that does not happen in time.
    pub async fn wait_until(&self, what: &str, condition: impl Fn(&Content) -> bool) -> Content {
        let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
        loop {
            let content = self.content();
            if condition(&content) {
                return content;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("timed out waiting until {what}; last seen: {content:#?}");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Waits until the server has closed the connection. Panics if it does not.
    pub async fn wait_until_closed(&self, what: &str) {
        let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
        while !self.reader.is_finished() {
            if tokio::time::Instant::now() >= deadline {
                panic!("timed out waiting until {what}");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Leaves the Page by closing the connection, and waits until it is closed.
    pub async fn disconnect(mut self) {
        let _ = self.outgoing.send(Message::Close(None));
        let _ = tokio::time::timeout(CONNECT_TIMEOUT, &mut self.writer).await;
        let _ = tokio::time::timeout(CONNECT_TIMEOUT, &mut self.reader).await;
    }

    /// Ends a local edit and sends it, as the editor does after each change.
    fn send_edit(&self, txn: yrs::TransactionMut) {
        let update = txn.encode_update_v1();
        drop(txn);
        let _ = self
            .outgoing
            .send(Message::Binary(frame(FRAME_UPDATE, &update).into()));
    }
}

impl Drop for TestClient {
    fn drop(&mut self) {
        self.reader.abort();
        self.writer.abort();
    }
}

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn open_socket(
    addr: SocketAddr,
    page: &str,
    cookie: Option<&str>,
) -> Result<(Socket, tungstenite::handshake::client::Response), tungstenite::Error> {
    let mut request = format!("ws://{addr}/ws/{page}").into_client_request()?;
    if let Some(cookie) = cookie {
        request
            .headers_mut()
            .insert("Cookie", cookie.parse().expect("a cookie is a valid header"));
    }
    tokio_tungstenite::connect_async(request).await
}

fn frame(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 1);
    frame.push(kind);
    frame.extend_from_slice(payload);
    frame
}

fn plain_text<T: ReadTxn>(txn: &T, text: &yrs::XmlTextRef) -> String {
    text.diff(txn, YChange::identity)
        .into_iter()
        .filter_map(|chunk| match chunk.insert {
            Out::Any(Any::String(s)) => Some(s.to_string()),
            _ => None,
        })
        .collect()
}

// Presence uses the encoding of y-protocols' awareness: a count, then per
// client its id, a clock and its state as a JSON string ("null" once gone).

fn own_presence_frame(shared: &Shared) -> Option<Vec<u8>> {
    let own = shared.own_presence.lock().unwrap();
    let (clock, state) = own.as_ref()?;
    let mut payload = Vec::new();
    write_varuint(&mut payload, 1);
    write_varuint(&mut payload, shared.doc.client_id().get());
    write_varuint(&mut payload, *clock);
    write_varuint(&mut payload, state.len() as u64);
    payload.extend_from_slice(state.as_bytes());
    Some(frame(FRAME_PRESENCE, &payload))
}

fn apply_presence(shared: &Shared, mut data: &[u8]) {
    let own_id = shared.doc.client_id().get();
    let mut others = shared.others.lock().unwrap();
    let count = read_varuint(&mut data).expect("unreadable presence");
    for _ in 0..count {
        let client = read_varuint(&mut data).expect("unreadable presence");
        let clock = read_varuint(&mut data).expect("unreadable presence");
        let len = read_varuint(&mut data).expect("unreadable presence") as usize;
        let state = String::from_utf8(data[..len].to_vec()).expect("presence is not UTF-8");
        data = &data[len..];
        if client == own_id {
            continue;
        }
        // As y-protocols does: only a newer clock counts, except that a
        // departure may carry the same clock as the last state.
        let accepted = others
            .get(&client)
            .is_none_or(|(known, _)| clock > *known || (clock == *known && state == "null"));
        if !accepted {
            continue;
        }
        if state == "null" {
            others.remove(&client);
        } else {
            others.insert(client, (clock, state));
        }
    }
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
