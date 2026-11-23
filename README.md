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
TURTLE_DATA=/var/lib/turtle TURTLE_PORT=8080 TURTLE_PUBLIC_URL=https://turtle.example.org ./server/target/release/turtle
```

Behind a reverse proxy, set `TURTLE_PUBLIC_URL` to the address people open in the browser. Turtle cannot work it out by itself, and without it AI clients cannot sign in; the **Connect AI** page then tells the Workspace Admin what is missing.

When a newer Turtle starts on an existing data directory, it brings the database up to date by itself. To back up Turtle, stop it and copy the data directory.

## Connect AI

With `TURTLE_PUBLIC_URL` set, a Member connects Claude or another MCP client by pasting `<address>/mcp` into it, signing in and clicking **Allow**. **Connect AI** in the account menu shows the address and the steps for Claude, Claude Code and other clients. Before clicking **Allow**, they can limit the connection to reading and to chosen Areas under **Limit access**. A connection never has more access than its Member, and what it changes shows as "<Member> via <app>".

Every Member sees what can act as them under **Connected apps** in the account menu: the apps they connected and the Tokens they created by hand, each with a **Disconnect** button. A Workspace Admin sees those of every Member under **All connected apps**, and can turn connecting AI off for the whole Workspace on the Connect AI page; Tokens created by hand keep working then.

For scripts and clients without OAuth, a Token is created by hand under **For developers** on Connected apps. It can be limited to reading, to chosen Areas and to an expiry date, and its value is shown once. The client sends it to the MCP endpoint, `/mcp` (Streamable HTTP), as `Authorization: Bearer <token>`. It finds one tool for every operation except the few that only make sense in a browser, such as signing in and managing Tokens. The same header works for the HTTP API, `POST /api/<operation>`.

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
