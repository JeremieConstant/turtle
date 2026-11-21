//! Pages that are open in memory while people edit them, and how they get
//! back into the database.

use crate::model::Block;
use crate::ops::Clock;
use anyhow::Context;
use rusqlite::{Connection, OptionalExtension};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use yrs::updates::decoder::Decode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

// The first byte of every editing frame says what the rest is.
/// A document update, in both directions.
pub const FRAME_UPDATE: u8 = 0;
/// Presence (who is here, where their cursor is). Relayed, never interpreted.
pub const FRAME_PRESENCE: u8 = 1;
/// Sent by the server when someone joins, asking the others to announce themselves.
pub const FRAME_PRESENCE_REQUEST: u8 = 2;

/// The connection a frame came from when Turtle itself made the change.
pub const FROM_TURTLE: u64 = 0;

pub fn frame(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 1);
    frame.push(kind);
    frame.extend_from_slice(payload);
    frame
}

/// How long changes may stay unrecorded while people keep editing: a Version
/// is recorded at the latest this many seconds after the first of them.
pub const VERSION_INTERVAL: i64 = 120;

/// A Page that is open in memory.
pub struct OpenPage {
    id: String,
    pub doc: Doc,
    /// Frames for everyone connected, tagged with the connection that sent them.
    pub frames: broadcast::Sender<(u64, Vec<u8>)>,
    /// Held while the content is changed or read for a Version, so that a
    /// change and its author are always recorded together.
    state: Mutex<PageState>,
    /// Held while a Version is recorded, so that recordings of the same Page
    /// never interleave: a restore and its Version stay one step.
    recording: Mutex<()>,
    /// The Agents that changed the Page, in the order they first did; each
    /// appears to the editors under a presence client of its own.
    agents: Mutex<Vec<Author>>,
    /// The clock of the latest presence sent for an Agent.
    agent_clock: Mutex<u64>,
}

/// Presence clients of Agents are numbered from here. Editors pick 32-bit
/// client ids, so these never collide with one.
const FIRST_AGENT_CLIENT: u64 = 1 << 32;

/// Who made a change: a Member, or an Agent acting through one of their Tokens.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Author {
    pub member_id: String,
    /// The Agent's name, when an Agent made the change.
    pub agent: Option<String>,
}

impl Author {
    pub fn member(member_id: &str) -> Author {
        Author { member_id: member_id.to_string(), agent: None }
    }
}


#[derive(Default)]
struct PageState {
    /// Changed since it was last written to the database.
    unsaved: bool,
    /// Editing connections open on the Page.
    connections: usize,
    /// Changes no Version holds yet.
    unrecorded: Option<Unrecorded>,
}

struct Unrecorded {
    /// When the first of them arrived.
    since: i64,
    /// Who made them.
    authors: BTreeSet<Author>,
}

/// Every open Page, with its own database connection so live editing does
/// not wait behind operations.
pub struct Pages {
    db: Mutex<Connection>,
    open: Mutex<HashMap<String, Arc<OpenPage>>>,
    clock: Clock,
}

impl Pages {
    pub fn new(db: Connection, clock: Clock) -> Pages {
        Pages { db: Mutex::new(db), open: Mutex::new(HashMap::new()), clock }
    }

    /// The Page, loaded from the database the first time it is asked for.
    /// Does not check that it exists or who may see it.
    pub fn open(&self, id: &str) -> anyhow::Result<Arc<OpenPage>> {
        let mut open = self.open.lock().unwrap();
        if let Some(page) = open.get(id) {
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
            id: id.to_string(),
            doc,
            frames,
            state: Mutex::new(PageState::default()),
            recording: Mutex::new(()),
            agents: Mutex::new(Vec::new()),
            agent_clock: Mutex::new(0),
        });
        open.insert(id.to_string(), page.clone());
        Ok(page)
    }

    /// Applies a Member's update from an editor to an open Page. Passing it
    /// on to the other editors is the caller's.
    pub fn apply(&self, page: &OpenPage, update: &[u8], author: &Author) -> anyhow::Result<()> {
        let update = Update::decode_v1(update).context("unreadable update")?;
        let mut state = page.state.lock().unwrap();
        page.doc.transact_mut().apply_update(update)?;
        self.changed(&mut state, author);
        Ok(())
    }

    /// Makes the content of an open Page equal to the given blocks, changing
    /// only what differs so that people editing at the same moment keep
    /// their work and their cursor. Connected editors receive the change.
    /// Says whether anything changed. Tests change Pages this way, as an
    /// editor would; operations change them through `change_as_version`.
    #[cfg(test)]
    pub fn write(&self, page: &OpenPage, blocks: &[Block], author: &Author) -> bool {
        let update = {
            let mut state = page.state.lock().unwrap();
            let Some(update) = crate::ydoc::write(&page.doc, blocks) else { return false };
            self.changed(&mut state, author);
            update
        };
        let _ = page.frames.send((FROM_TURTLE, frame(FRAME_UPDATE, &update)));
        true
    }

    /// Notes a change, to be saved and recorded.
    fn changed(&self, state: &mut PageState, author: &Author) {
        let now = (self.clock)();
        state.unsaved = true;
        state
            .unrecorded
            .get_or_insert_with(|| Unrecorded { since: now, authors: BTreeSet::new() })
            .authors
            .insert(author.clone());
    }

    /// Says that an editing connection opened on the Page.
    pub fn connected(&self, page: &OpenPage) {
        page.state.lock().unwrap().connections += 1;
    }

    /// Says that an editing connection closed. When it was the last one, the
    /// changes since the previous Version are recorded.
    pub fn disconnected(&self, page: &OpenPage) -> anyhow::Result<()> {
        let last = {
            let mut state = page.state.lock().unwrap();
            state.connections = state.connections.saturating_sub(1);
            state.connections == 0
        };
        if last {
            self.record(page)?;
        }
        Ok(())
    }

    /// Records the unrecorded changes of every Page whose first unrecorded
    /// change is `VERSION_INTERVAL` old.
    pub fn record_due(&self) -> anyhow::Result<()> {
        let now = (self.clock)();
        each_open(self.all_open(), |_, page| {
            let due = page
                .state
                .lock()
                .unwrap()
                .unrecorded
                .as_ref()
                .is_some_and(|unrecorded| now - unrecorded.since >= VERSION_INTERVAL);
            if due {
                self.record(page)?;
            }
            Ok(())
        })
    }

    /// Records the unrecorded changes of every Page, as Turtle stops.
    pub fn record_all(&self) -> anyhow::Result<()> {
        each_open(self.all_open(), |_, page| self.record(page).map(|_| ()))
    }

    /// Changes an open Page as `change` makes of its content, changing only
    /// what differs, as a Version of its own. The changes made before are
    /// recorded first, so the history keeps them apart. Connected editors
    /// receive the change, and are told which Agent made it. Returns the
    /// number of the new Version, or nothing when the Page already said
    /// that; or why `change` refused, in which case nothing happens.
    pub fn change_as_version<E>(
        &self,
        page: &OpenPage,
        by: &Author,
        restored_from: Option<u32>,
        change: impl FnOnce(Vec<Block>) -> Result<Vec<Block>, E>,
    ) -> anyhow::Result<Result<Option<u32>, E>> {
        let _recording = page.recording.lock().unwrap();
        // Read, changed and read again in one step: an edit arriving meanwhile
        // is neither undone nor counted as this change.
        let (before, taken, after, update) = {
            let mut state = page.state.lock().unwrap();
            let blocks = crate::ydoc::read(&page.doc);
            let before = crate::markdown::to_markdown(&blocks);
            let target = match change(blocks) {
                Ok(target) => target,
                Err(refused) => return Ok(Err(refused)),
            };
            let update = crate::ydoc::write(&page.doc, &target);
            state.unsaved |= update.is_some();
            let after = crate::markdown::to_markdown(&crate::ydoc::read(&page.doc));
            (before, state.unrecorded.take(), after, update)
        };
        if let Some(update) = update {
            let _ = page.frames.send((FROM_TURTLE, frame(FRAME_UPDATE, &update)));
            if let Some(name) = &by.agent {
                announce_agent(page, by, name);
            }
        }
        let now = (self.clock)();
        if let Some(taken) = taken {
            if let Err(error) = self.insert_version(&page.id, &before, now, None, &taken.authors) {
                // Both wait for the next round, together.
                let mut authors = taken.authors;
                authors.insert(by.clone());
                self.put_back(page, Unrecorded { since: taken.since, authors });
                return Err(error);
            }
        }
        let recorded = self.insert_version(&page.id, &after, now, restored_from, &BTreeSet::from([by.clone()]));
        if recorded.is_err() {
            self.put_back(page, Unrecorded { since: now, authors: BTreeSet::from([by.clone()]) });
        }
        recorded.map(Ok)
    }

    /// Records the Page as it is now as a new Version holding the changes no
    /// Version holds yet, unless its content is that of the previous Version.
    fn record(&self, page: &OpenPage) -> anyhow::Result<Option<u32>> {
        let _recording = page.recording.lock().unwrap();
        self.record_holding(page)
    }

    /// `record` for a caller that holds `page.recording`.
    fn record_holding(&self, page: &OpenPage) -> anyhow::Result<Option<u32>> {
        // The changes and the content are taken together, so a change that
        // arrives meanwhile goes wholly into the next Version.
        let (taken, markdown) = {
            let mut state = page.state.lock().unwrap();
            let Some(taken) = state.unrecorded.take() else { return Ok(None) };
            (taken, crate::markdown::to_markdown(&crate::ydoc::read(&page.doc)))
        };
        let recorded = self.insert_version(&page.id, &markdown, (self.clock)(), None, &taken.authors);
        if recorded.is_err() {
            self.put_back(page, taken);
        }
        recorded
    }

    /// Puts changes that could not be recorded back, so the next round
    /// records them with whatever arrived meanwhile.
    fn put_back(&self, page: &OpenPage, taken: Unrecorded) {
        let mut state = page.state.lock().unwrap();
        match &mut state.unrecorded {
            Some(newer) => {
                newer.since = taken.since;
                newer.authors.extend(taken.authors);
            }
            None => state.unrecorded = Some(taken),
        }
    }

    fn insert_version(
        &self,
        page_id: &str,
        markdown: &str,
        now: i64,
        restored_from: Option<u32>,
        authors: &BTreeSet<Author>,
    ) -> anyhow::Result<Option<u32>> {
        let mut db = self.db.lock().unwrap();
        let transaction = db.transaction()?;
        let previous: Option<(u32, String)> = transaction
            .query_row(
                "SELECT number, markdown FROM versions WHERE page_id = ?1 ORDER BY number DESC LIMIT 1",
                [page_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        // A Page that has never been recorded starts out empty.
        let (last_number, last_markdown) = previous.unwrap_or((0, String::new()));
        if last_markdown == markdown {
            return Ok(None);
        }
        let number = last_number + 1;
        transaction.execute(
            "INSERT INTO versions (page_id, number, markdown, recorded_at, restored_from)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![page_id, number, markdown, now, restored_from],
        )?;
        for author in authors {
            transaction.execute(
                "INSERT INTO version_authors (page_id, number, member_id, agent) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![page_id, number, author.member_id, author.agent.as_deref().unwrap_or_default()],
            )?;
        }
        transaction.commit()?;
        Ok(Some(number))
    }

    /// Writes every changed Page to the database, calls `saved` with the ID
    /// of each, and returns how many there were.
    pub fn save_changed(&self, saved: impl Fn(&str)) -> anyhow::Result<usize> {
        let mut count = 0;
        each_open(self.all_open(), |id, page| {
            if !std::mem::take(&mut page.state.lock().unwrap().unsaved) {
                return Ok(());
            }
            let state = page.doc.transact().encode_state_as_update_v1(&StateVector::default());
            let written = self.db.lock().unwrap().execute(
                "INSERT INTO pages (id, state) VALUES (?1, ?2)
                 ON CONFLICT (id) DO UPDATE SET state = excluded.state",
                rusqlite::params![id, state],
            );
            if let Err(error) = written {
                // Keep the page marked so the next round tries again.
                page.state.lock().unwrap().unsaved = true;
                return Err(error.into());
            }
            saved(id);
            count += 1;
            Ok(())
        })?;
        Ok(count)
    }

    fn all_open(&self) -> Vec<(String, Arc<OpenPage>)> {
        self.open
            .lock()
            .unwrap()
            .iter()
            .map(|(id, page)| (id.clone(), page.clone()))
            .collect()
    }
}

/// Tells the editors which Agent just changed the Page, as presence without
/// a cursor: Agents have none. Editors forget it after a while unless the
/// Agent writes again, which is what an Agent being there means.
fn announce_agent(page: &OpenPage, author: &Author, name: &str) {
    let client = {
        let mut agents = page.agents.lock().unwrap();
        let index = match agents.iter().position(|known| known == author) {
            Some(index) => index,
            None => {
                agents.push(author.clone());
                agents.len() - 1
            }
        };
        FIRST_AGENT_CLIENT + index as u64
    };
    let clock = {
        let mut clock = page.agent_clock.lock().unwrap();
        *clock += 1;
        *clock
    };
    let state = serde_json::json!({ "user": { "name": name, "agent": true } }).to_string();
    let mut payload = Vec::new();
    write_varuint(&mut payload, 1);
    write_varuint(&mut payload, client);
    write_varuint(&mut payload, clock);
    write_varuint(&mut payload, state.len() as u64);
    payload.extend_from_slice(state.as_bytes());
    let _ = page.frames.send((FROM_TURTLE, frame(FRAME_PRESENCE, &payload)));
}

pub fn read_varuint(data: &mut &[u8]) -> Option<u64> {
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

pub fn write_varuint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// Does something for each open Page. One that fails does not keep the
/// others from their turn; the first failure is returned once all had it.
fn each_open(
    pages: Vec<(String, Arc<OpenPage>)>,
    mut each: impl FnMut(&str, &OpenPage) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let mut first_failure = None;
    for (id, page) in pages {
        if let Err(error) = each(&id, &page) {
            first_failure.get_or_insert(error.context(format!("Page {id}")));
        }
    }
    first_failure.map_or(Ok(()), Err)
}
