//! The load test: many people editing at the same time on one node with
//! SQLite (ADR-0004). It reuses the live-sync harness, so every simulated
//! editor is a real Member, signed in, speaking the web editor's protocol.
//!
//! It runs on demand only (`scripts/load-test.sh`), never with the normal
//! test command. Settings come from the environment:
//!
//! - `TURTLE_LOAD_EDITORS`: people editing at the same time (500)
//! - `TURTLE_LOAD_PAGES`: Pages they are spread over (100)
//! - `TURTLE_LOAD_SECONDS`: how long they edit (60)
//! - `TURTLE_LOAD_KEYSTROKE_MS`: average time between one editor's keystrokes (500)
//!
//! Each editor types into a paragraph of its own, one character per change,
//! as the web editor sends them. The test reports how long a change takes to
//! reach the other editors of the same Page, the server's memory and how
//! long saving takes, checks that everyone on a Page ends with the same
//! content, and fails if Turtle did not keep up.
#![cfg(unix)]

mod support;

use futures_util::{stream, StreamExt};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use support::{Member, TestClient, TestServer};

/// Areas the Pages are spread over.
const AREAS: usize = 10;
/// How many invitations, connections and so on are in flight at once while
/// setting up, so setting up does not become a load test of its own.
const SETUP_CONCURRENCY: usize = 32;
/// Accepting an invitation hashes a password, which takes memory (Argon2);
/// many at once would inflate the memory the report shows for editing.
const INVITE_CONCURRENCY: usize = 4;
const TEXT: &str = "the quick brown fox jumps over the lazy dog ";

/// What "keeps up" means. A change reaches the others well under a second,
/// so people see each other type; a save takes well under the two-second
/// save interval, so saves never pile up.
const LATENCY_P99_LIMIT: Duration = Duration::from_millis(500);
const SAVE_LIMIT: Duration = Duration::from_secs(1);

struct Settings {
    editors: usize,
    pages: usize,
    duration: Duration,
    keystroke: Duration,
}

impl Settings {
    fn from_env() -> Settings {
        let read = |name: &str, default: u64| -> u64 {
            match std::env::var(name) {
                Ok(value) => value
                    .parse()
                    .unwrap_or_else(|_| panic!("{name} should be a whole number, not {value:?}")),
                Err(_) => default,
            }
        };
        let settings = Settings {
            editors: read("TURTLE_LOAD_EDITORS", 500) as usize,
            pages: read("TURTLE_LOAD_PAGES", 100) as usize,
            duration: Duration::from_secs(read("TURTLE_LOAD_SECONDS", 60)),
            keystroke: Duration::from_millis(read("TURTLE_LOAD_KEYSTROKE_MS", 500)),
        };
        assert!(settings.pages > 0, "TURTLE_LOAD_PAGES should be at least 1");
        assert!(
            settings.editors >= 2 * settings.pages,
            "every Page needs at least two editors for changes to reach someone"
        );
        settings
    }
}

/// When each change was sent, and how long it took to reach each other
/// editor of its Page.
#[derive(Default)]
struct Deliveries {
    /// (client id, clock the change ends at) to when it was sent.
    sent: HashMap<(u64, u32), Instant>,
    latencies: Vec<Duration>,
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "a load test; run it with scripts/load-test.sh"]
async fn many_people_editing_at_the_same_time() {
    let settings = Settings::from_env();
    check_open_file_limit(settings.editors);
    let mut server = TestServer::start_reporting_saves().await;
    let admin = server.sign_up().await;

    println!(
        "Setting up {} Members on {} Pages...",
        settings.editors, settings.pages
    );
    let pages = create_pages(&admin, settings.pages).await;
    let members: Vec<Member> = stream::iter(0..settings.editors)
        .map(|i| {
            let (server, admin) = (&server, &admin);
            async move { server.invite(admin, &format!("Editor{i}")).await }
        })
        .buffered(INVITE_CONCURRENCY)
        .collect()
        .await;
    let editors: Vec<Editor> = stream::iter(members.iter().enumerate())
        .map(|(i, member)| {
            let page = pages[i % pages.len()].clone();
            let server = &server;
            async move {
                Editor {
                    label: format!("Editor {i}: "),
                    typed: String::new(),
                    client: server.connect(member, &page).await,
                    page,
                    paragraph: 0,
                    random_state: i as u64 + 1,
                }
            }
        })
        .buffered(SETUP_CONCURRENCY)
        .collect()
        .await;
    let mut editors = find_own_paragraphs(editors).await;

    let deliveries = Arc::new(Mutex::new(Deliveries::default()));
    for editor in &editors {
        let deliveries = deliveries.clone();
        editor.client.on_update(move |reaches| {
            let arrived = Instant::now();
            let mut deliveries = deliveries.lock().unwrap();
            for change in reaches {
                if let Some(sent) = deliveries.sent.get(change).copied() {
                    deliveries.latencies.push(arrived - sent);
                }
            }
        });
    }
    // Saves from setting up do not count.
    server.take_saves().await;

    println!("Editing for {:?}...", settings.duration);
    let started = Instant::now();
    let memory = sample_memory(&server, settings.duration).await;
    let ends = tokio::time::Instant::now() + settings.duration;
    let typing: Vec<_> = editors
        .drain(..)
        .map(|editor| {
            tokio::spawn(type_until(editor, ends, settings.keystroke, deliveries.clone()))
        })
        .collect();
    let mut editors = Vec::new();
    for task in typing {
        editors.push(task.await.expect("an editor stopped typing"));
    }
    let editing = started.elapsed();
    let memory = memory.await.expect("sampling memory");

    println!("Checking that every editor of a Page has the same content...");
    let changes: usize = editors.iter().map(|editor| editor.typed.chars().count()).sum();
    for page in &pages {
        let on_page: Vec<&Editor> = editors.iter().filter(|editor| &editor.page == page).collect();
        let mut expected: Vec<String> = on_page
            .iter()
            .map(|editor| format!("{}{}", editor.label, editor.typed))
            .collect();
        expected.sort();
        let mut documents = Vec::new();
        for editor in &on_page {
            let content = editor
                .client
                .wait_until(&format!("{} has everyone's changes", editor.label), |content| {
                    let mut seen = content.paragraphs.clone();
                    seen.sort();
                    seen == expected
                })
                .await;
            documents.push(content.document);
        }
        assert!(
            documents.windows(2).all(|pair| pair[0] == pair[1]),
            "the editors of {page} ended with different content"
        );
        let late = server.connect(&admin, page).await;
        assert_eq!(late.content().document, documents[0], "the server holds other content for {page}");
        late.disconnect().await;
    }
    // Once everyone has everything, the last save is the one of the last changes.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let saves = server.take_saves().await;

    let mut latencies = std::mem::take(&mut deliveries.lock().unwrap().latencies);
    let expected_deliveries: usize = editors
        .iter()
        .map(|editor| {
            let others = editors.iter().filter(|other| other.page == editor.page).count() - 1;
            editor.typed.chars().count() * others
        })
        .sum();
    latencies.sort();
    let mut save_times: Vec<Duration> = saves.iter().map(|save| save.took).collect();
    save_times.sort();

    println!();
    println!("== Load test");
    println!(
        "{} editors (each a Member) on {} Pages, a keystroke every {:?} on average, for {:.1} s",
        settings.editors,
        settings.pages,
        settings.keystroke,
        editing.as_secs_f64()
    );
    println!(
        "Changes: {changes} ({:.0} per second), deliveries to other editors: {} of {expected_deliveries}",
        changes as f64 / editing.as_secs_f64(),
        latencies.len()
    );
    println!(
        "Change reaches the other editors: median {}, p95 {}, p99 {}, max {}",
        ms(percentile(&latencies, 50)),
        ms(percentile(&latencies, 95)),
        ms(percentile(&latencies, 99)),
        ms(percentile(&latencies, 100)),
    );
    println!(
        "Server memory: {} after setting up, {} at most, {} at the end",
        mib(memory.first),
        mib(memory.peak),
        mib(memory.last)
    );
    println!(
        "Saves: {}, Pages per save at most {}, took median {}, p95 {}, max {}",
        saves.len(),
        saves.iter().map(|save| save.pages).max().unwrap_or(0),
        ms(percentile(&save_times, 50)),
        ms(percentile(&save_times, 95)),
        ms(percentile(&save_times, 100)),
    );
    println!("Content: every editor of each Page ended with the same content.");

    let all_delivered = latencies.len() == expected_deliveries;
    let fast_enough = percentile(&latencies, 99) < LATENCY_P99_LIMIT;
    let saves_keep_up = percentile(&save_times, 100) < SAVE_LIMIT;
    let keeps_up = all_delivered && fast_enough && saves_keep_up;
    println!(
        "Keeps up: {} (every change delivered: {all_delivered}; p99 under {}: {fast_enough}; every save under {}: {saves_keep_up})",
        if keeps_up { "yes" } else { "no" },
        ms(LATENCY_P99_LIMIT),
        ms(SAVE_LIMIT),
    );
    println!();
    server.stop().await;
    assert!(keeps_up, "Turtle did not keep up; see the report above");
}

struct Editor {
    client: TestClient,
    page: String,
    /// What its paragraph starts with, to find it.
    label: String,
    /// Its paragraph's position, fixed once every editor has added theirs.
    paragraph: u32,
    /// Everything it typed into its paragraph after the label.
    typed: String,
    /// The state of its generator for the pauses between keystrokes;
    /// different for every editor.
    random_state: u64,
}

/// One Page per Area in turn, every Area open to all Members with Write.
async fn create_pages(admin: &Member, count: usize) -> Vec<String> {
    let mut areas = Vec::new();
    for i in 0..AREAS.min(count) {
        let area = admin.call("create_area", json!({ "name": format!("Area {i}") })).await;
        admin
            .call(
                "set_grant",
                json!({ "area_id": area["id"], "holder": { "kind": "all_members" }, "role": "write" }),
            )
            .await;
        areas.push(area["id"].clone());
    }
    let mut pages = Vec::new();
    for i in 0..count {
        let page = admin
            .call("create_page", json!({ "area_id": areas[i % areas.len()], "title": format!("Page {i}") }))
            .await;
        pages.push(page["id"].as_str().expect("a Page has an ID").to_string());
    }
    pages
}

/// Every editor adds its paragraph; once all editors of a Page see all of
/// them, nobody adds or removes paragraphs any more, so positions are fixed.
async fn find_own_paragraphs(mut editors: Vec<Editor>) -> Vec<Editor> {
    let mut on_page: HashMap<String, usize> = HashMap::new();
    for editor in &editors {
        editor.client.append_paragraph(&editor.label);
        *on_page.entry(editor.page.clone()).or_default() += 1;
    }
    for editor in &mut editors {
        let count = on_page[&editor.page];
        let content = editor
            .client
            .wait_until("every editor of the Page has added a paragraph", |content| {
                content.paragraphs.len() == count
            })
            .await;
        let position = content
            .paragraphs
            .iter()
            .position(|paragraph| paragraph == &editor.label)
            .expect("an editor's own paragraph is there");
        editor.paragraph = position as u32;
    }
    editors
}

async fn type_until(
    mut editor: Editor,
    ends: tokio::time::Instant,
    keystroke: Duration,
    deliveries: Arc<Mutex<Deliveries>>,
) -> Editor {
    let mut text = TEXT.chars().cycle().skip(editor.random_state as usize % TEXT.len());
    loop {
        // Between half and one and a half times the average, so editors
        // do not type in lockstep.
        let pause = keystroke.mul_f64(0.5 + next_random(&mut editor.random_state));
        if tokio::time::Instant::now() + pause >= ends {
            return editor;
        }
        tokio::time::sleep(pause).await;
        let character = text.next().unwrap();
        // One character is one more step of this client's clock.
        let clock = editor.client.own_clock() + 1;
        deliveries
            .lock()
            .unwrap()
            .sent
            .insert((editor.client.client_id(), clock), Instant::now());
        let offset = (editor.label.chars().count() + editor.typed.chars().count()) as u32;
        editor.client.type_text(editor.paragraph, offset, &character.to_string());
        editor.typed.push(character);
    }
}

/// A number in [0, 1), from a xorshift generator: enough to spread pauses,
/// and the same on every run.
fn next_random(state: &mut u64) -> f64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 11) as f64 / (1u64 << 53) as f64
}

struct Memory {
    first: u64,
    peak: u64,
    last: u64,
}

/// Samples the server's memory once a second while the editors type.
async fn sample_memory(server: &TestServer, duration: Duration) -> tokio::task::JoinHandle<Memory> {
    let read_memory = server.memory_reader();
    tokio::spawn(async move {
        let first = read_memory();
        let mut memory = Memory { first, peak: first, last: first };
        let ends = tokio::time::Instant::now() + duration;
        while tokio::time::Instant::now() < ends {
            tokio::time::sleep(Duration::from_secs(1)).await;
            memory.last = read_memory();
            memory.peak = memory.peak.max(memory.last);
        }
        memory
    })
}

/// Every connection is an open file on both ends, and the system's default
/// limit is often far below what 500 editors need.
fn check_open_file_limit(editors: usize) {
    let output = std::process::Command::new("sh")
        .args(["-c", "ulimit -n"])
        .output()
        .expect("asking for the open file limit");
    let limit = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let needed = editors + 256;
    if limit != "unlimited" {
        let limit: usize = limit.parse().expect("ulimit -n prints a number");
        assert!(
            limit >= needed,
            "{editors} editors need at least {needed} open files, but the limit is {limit}. \
             Run the load test with scripts/load-test.sh, which raises it."
        );
    }
}

fn percentile(sorted: &[Duration], percent: usize) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let index = (sorted.len() * percent).div_ceil(100).clamp(1, sorted.len()) - 1;
    sorted[index]
}

fn ms(duration: Duration) -> String {
    format!("{:.1} ms", duration.as_secs_f64() * 1000.0)
}

fn mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}
