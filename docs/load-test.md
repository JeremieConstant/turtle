# Load test

`scripts/load-test.sh` runs `server/tests/load.rs`: many people editing at the same time against one release build of Turtle with SQLite, as ADR-0004 assumes. It is not part of `scripts/check.sh`.

## What it does

- Starts the real `turtle` binary with its own data directory, as the live-sync tests do, and reuses their client, which speaks the web editor's protocol.
- Invites one Member per simulated editor. Each editor signs in as its Member and opens its Page over WebSocket. Every change passes the server's access check, as a person's would.
- Spreads the editors evenly over the Pages, and the Pages over 10 Areas that all Members can write to.
- Each editor adds a paragraph of its own, then types into it one character per change, as the web editor sends keystrokes. The pause between keystrokes is random, between half and one and a half times the average.
- Measures:
  - the time from sending a change to its arrival, applied, at each other editor of the Page;
  - the server's resident memory, once a second while people edit;
  - how long each save takes, as the server reports with `TURTLE_LOG_SAVES` set.
- Afterwards it waits until every editor has every change. It then checks that all editors of a Page hold exactly the same document, and that someone who opens the Page then gets that document too.
- Fails if Turtle did not keep up, meaning any of these is not true:
  - every change reached every other editor of its Page;
  - 99 % of changes arrived within 500 ms;
  - every save took under 1 s, half the save interval.

The load is set through the environment:

| Variable | Meaning | Default |
| --- | --- | --- |
| `TURTLE_LOAD_EDITORS` | people editing at the same time | 500 |
| `TURTLE_LOAD_PAGES` | Pages they are spread over | 100 |
| `TURTLE_LOAD_SECONDS` | how long they edit | 60 |
| `TURTLE_LOAD_KEYSTROKE_MS` | average time between one editor's keystrokes | 500 |

The default is deliberately harsh: everyone typing two characters a second without pause, for a minute, adds up to about 1,000 changes a second. People who are "editing" mostly read, think and pause.

## Run of 2026-11-21

- Hardware: Apple M5 Pro (15 cores), 48 GB memory, macOS 27.0.1, internal SSD.
- Software: Rust 1.100.0, release build.
- Setup: the server and all 500 clients ran on this one machine and talked over loopback. The clients took CPU from the server, but there was no network latency.
- Load: the defaults, 500 editors on 100 Pages (5 per Page), a keystroke every 500 ms on average, for 60 s.

| Measure | Result |
| --- | --- |
| Changes | 59,876 (998 per second) |
| Deliveries to the other editors | 239,504 of 239,504 |
| Change reaches the other editors | median 0.4 ms, p95 1.1 ms, p99 6.8 ms, max 54.9 ms |
| Server memory | 174.7 MiB after setting up, 175.3 MiB at most, 99.7 MiB at the end |
| Saves | 31, writing up to 100 Pages each; median 6.5 ms, p95 8.2 ms, max 9.8 ms |
| Content | every editor of each Page ended with the same content, and the server held it too |

Verdict: Turtle kept up, with a wide margin on every measure.

Memory after setting up includes what accepting 500 invitations left behind: hashing a password with Argon2 takes memory, which the allocator returned later. While people edited, memory barely grew.

## What it does not cover

- **Network latency between people and the server.** The measured latency is the server's share only.
- **Many people typing in the same paragraph, or making structural edits.** Each editor typed only into its own paragraph, so expected contents are known exactly.
- **Exact timing.** All typists and observers record into one shared table, and the clients share the CPU with the server; both add a little to the measured latency.
- **Long-lived documents.** The run lasted a minute; a Page's document history keeps growing as people edit.
- **Load that is not editing.** The run did not include signing in, the tree, the Markdown API, or reconnecting after the server dropped a lagging connection.
