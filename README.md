# Turtle

Turtle is an open-source, self-hosted knowledge base where humans and agents write documentation together in real time. It runs as a single binary with one data directory: the web app is built into the binary and the database lives in the data directory, so nothing else is needed on disk.

Turtle is at an early stage. [docs/mvp.md](docs/mvp.md) describes where it is heading, [GLOSSARY.md](GLOSSARY.md) defines its terms, and [docs/adr/](docs/adr/) records the decisions behind it.

## Build

You need [Rust](https://rustup.rs) and [Node.js](https://nodejs.org) 22.18 or newer. All commands run from the repository root.

Build the web app first, then the server, which embeds it:

```bash
npm --prefix web ci
```

```bash
npm --prefix web run build
```

The web app's types are generated from the server, so this step calls `cargo` as well. Set `CARGO` to the cargo binary if it is not on the `PATH`.

```bash
cargo build --release --manifest-path server/Cargo.toml
```

The result is `server/target/release/turtle`. It can be copied anywhere and started on its own.

## Start

```bash
./server/target/release/turtle
```

Turtle then listens on <http://127.0.0.1:4100>. On the first visit it asks you to create the first account, which becomes the Workspace Admin.

| Setting | Default | Meaning |
| --- | --- | --- |
| `TURTLE_DATA` | `turtle-data` in the directory Turtle is started from | The data directory. It is created on the first start and holds the database, `turtle.db`, and the Attachments, each under the hash of its content in `attachments/`. |
| `TURTLE_PORT` | `4100` | The port Turtle listens on. |
| `TURTLE_MAX_ATTACHMENT_MB` | `25` | The largest file, in MB, that can be attached to a Page. |
| `TURTLE_PUBLIC_URL` | unset | The address people and AI clients reach Turtle at, such as `https://turtle.example.org`, at the root of its host. When set, Claude and other MCP clients can connect with OAuth: a Member pastes `<address>/mcp` into the client and clicks **Allow**. Without it, only Tokens created by hand work at the MCP endpoint. |
| `TURTLE_LOG_SAVES` | unset | When set, Turtle prints a line for each save of changed Pages and how long it took, to see whether saving keeps up. |

Settings are environment variables:

```bash
TURTLE_DATA=/var/lib/turtle TURTLE_PORT=8080 ./server/target/release/turtle
```

When a newer Turtle starts on an existing data directory, it brings the database up to date by itself. To back up Turtle, stop it and copy the data directory.

## Connect an Agent

Every Member sees what can act as them under **Connected apps** in the account menu: the apps they connected and the Tokens they created by hand, each with a **Disconnect** button. Tokens are created by hand there, under **For developers**. A Token can be limited to reading, to chosen Areas and to an expiry date, and never has more access than its owner. Its value is shown once.

An Agent connects to Turtle's built-in MCP endpoint, `/mcp` on the address Turtle is reached at (Streamable HTTP), and sends the Token as `Authorization: Bearer <token>`. It finds one tool for every operation except the few that only make sense in a browser, such as signing in and managing Tokens. The same header works for the HTTP API, `POST /api/<operation>`.

## Develop

Run the server and its tests from the repository root:

```bash
cargo run --manifest-path server/Cargo.toml
```

```bash
cargo test --manifest-path server/Cargo.toml
```

A debug build embeds whatever `web/dist` holds when it is compiled, and starts without a web app if that directory is missing. After changing the web app, run `npm --prefix web run build` and build the server again. `npm --prefix web test` runs the web app's tests.

`scripts/check.sh` runs every check CI runs: lints, the generated operation types, the web app's type check and tests, and the server's tests. To run it before each commit, enable the hook once per clone:

```bash
git config core.hooksPath .githooks
```

`scripts/load-test.sh` runs the load test on demand: 500 people editing at the same time against a release build, with a report on how fast changes reach the others, memory and saving. The last recorded run is in [docs/load-test.md](docs/load-test.md).

Licensed under [AGPL-3.0](LICENSE).
