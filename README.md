# Turtle Wiki

A self-hosted wiki where people and AI agents write documentation together.

**[Try the demo](https://demo.turtle-wiki.app)**: no sign-up, reset every 6 hours. · [turtle-wiki.app](https://turtle-wiki.app)

![A runbook being edited in Turtle, last changed by "Ada Example via Claude"](docs/screenshot.png)

**No feature behind a paywall.** Turtle is licensed under [AGPL-3.0](LICENSE), with no enterprise edition and no CLA: every feature is in this repository, and contributions stay AGPL.

- **One binary, nothing else to run.** The web app is built into the binary, and the SQLite database and the Attachments live in one data directory. No database server, no outside services.
- **An Agent never has more access than its person.** Agents work through MCP with almost every operation people have. An Agent a Member connects is limited to what that Member may do, and what it changes shows under their name, as "Ada Example via Claude" in the screenshot above.
- **Your writing stays Markdown.** Every Page can be read and written as Markdown, and an Area or Subtree exports as a ZIP of Markdown files with its Attachments, or as a PDF.

Turtle is at an early stage. [ROADMAP.md](ROADMAP.md) says what is being built now and what comes next, [GLOSSARY.md](GLOSSARY.md) defines its terms, and [docs/adr/](docs/adr/) records the decisions behind it.

## Run it

With [Docker](https://docs.docker.com/get-started/get-docker/), save [compose.yml](compose.yml) in a directory of its own and run there:

```bash
docker compose up -d
```

Turtle then listens on <http://localhost:4100>. On the first visit it asks you to create the first account, which becomes the Workspace Admin. Everything Turtle keeps is in the `turtle-data` volume, so it survives stopping, removing and updating the container. To update, pull the newer image and start it:

```bash
docker compose pull && docker compose up -d
```

The image is `ghcr.io/turtle-wiki/turtle`: `latest` is the newest release, `X.Y` and `X.Y.Z` a release line or one release, and `edge` whatever is on `main`. Settings from [Start](#start) go under `environment:` in `compose.yml`; set `TURTLE_PUBLIC_URL` there once Turtle has an address of its own. [Connect AI](#connect-ai) says how to let Claude or another MCP client in.

Without Docker, Turtle is one file built from source, as [Build](#build) describes:

```bash
./server/target/release/turtle
```

It then listens on <http://127.0.0.1:4100>.

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

Turtle is configured with environment variables:

| Setting | Default | Meaning |
| --- | --- | --- |
| `TURTLE_DATA` | `turtle-data` in the directory Turtle is started from | The data directory. It is created on the first start and holds the database, `turtle.db`, and the Attachments, each under the hash of its content in `attachments/`. |
| `TURTLE_HOST` | `127.0.0.1` | The IP address Turtle listens on. The default lets in only connections from the same machine; `0.0.0.0` lets in every IPv4 address, `::` every address. The container image sets `0.0.0.0`. |
| `TURTLE_PORT` | `4100` | The port Turtle listens on. |
| `TURTLE_MAX_ATTACHMENT_MB` | `25` | The largest file, in MB, that can be attached to a Page. |
| `TURTLE_PUBLIC_URL` | unset | The address people and AI clients reach Turtle at, such as `https://turtle.example.org`, at the root of its host. When set, Claude and other MCP clients can connect with OAuth: a Member pastes `<address>/mcp` into the client and clicks **Allow**. Without it, only Tokens created by hand work at the MCP endpoint. |
| `TURTLE_CLIENT_ADDRESS_HEADER` | unset | Behind a reverse proxy, the header it passes on the client's address in, such as `X-Forwarded-For`. Turtle takes the last address in it, the one the proxy added; behind more than one proxy, name the header the outermost one sets, such as `CF-Connecting-IP` behind Cloudflare. Someone who fails to sign in or to use a single-use link too often in a short time is turned away for 15 minutes, and so is everyone trying an email address after five failed attempts with it; without this setting, everyone behind the proxy counts as one. Leave it unset without a proxy, since anyone can send the header. |
| `TURTLE_SMTP_URL` | unset | The SMTP server Turtle sends mail through, such as `smtps://user:password@smtp.example.org` or `smtp://user:password@smtp.example.org:587?tls=required`. When set, invitations and password reset links are mailed, and a Member who forgot their password can ask for a link on the sign-in screen. Needs `TURTLE_MAIL_FROM` and `TURTLE_PUBLIC_URL`. Without it, a Workspace Admin gets each link to pass on by hand. |
| `TURTLE_MAIL_FROM` | unset | The address Turtle's mail comes from, such as `Turtle <turtle@example.org>`. |
| `TURTLE_DEMO_EMAIL` | unset | Makes Turtle a public demo, together with `TURTLE_DEMO_PASSWORD`: the email address of the demo Member, a Member without Workspace Admin whom anyone may sign in as. Every screen then says that this is a demo. |
| `TURTLE_DEMO_PASSWORD` | unset | The demo Member's password. It is shown to everyone. |
| `TURTLE_DEMO_RESET_HOURS` | unset | How many hours pass between resets of the demo, a whole number, for the notice on every screen. Only in demo mode. |
| `TURTLE_LOG_SAVES` | unset | When set, Turtle prints a line for each save of changed Pages and how long it took, to see whether saving keeps up. |

For example:

```bash
TURTLE_DATA=/var/lib/turtle TURTLE_PORT=8080 TURTLE_PUBLIC_URL=https://turtle.example.org ./server/target/release/turtle
```

The demo account must be a Member without Workspace Admin; Turtle never shows a Workspace Admin's password, and shows no account until a Member with that email address exists. Turtle does not reset the demo itself: the hosting stops it, empties the data directory, starts it and fills it again.

Behind a reverse proxy, set `TURTLE_PUBLIC_URL` to the address people open in the browser. Turtle cannot work it out by itself, and without it AI clients cannot sign in; the **Connect AI** page then tells the Workspace Admin what is missing. Set `TURTLE_CLIENT_ADDRESS_HEADER` too, so Turtle tells the people behind the proxy apart when it counts failed attempts to sign in.

When a newer Turtle starts on an existing data directory, it brings the database up to date by itself. To back up Turtle, stop it and copy the data directory. With Docker, the data directory is the `turtle-data` volume.

## Connect AI

With `TURTLE_PUBLIC_URL` set, a Member connects Claude or another MCP client by pasting `<address>/mcp` into it, signing in and clicking **Allow**. **Connect AI** in the account menu shows the address and the steps for Claude, Claude Code and other clients. Before clicking **Allow**, they can limit the connection to reading and to chosen Areas under **Limit access**. A connection never has more access than its Member, and what it changes shows as "`<Member>` via `<app>`".

Every Member sees what can act as them under **Connected apps** in the account menu: the apps they connected and the Tokens they created by hand, each with a **Disconnect** button. A Workspace Admin sees those of every Member under **All connected apps**, and can turn connecting AI off for the whole Workspace on the Connect AI page; Tokens created by hand keep working then.

For scripts and clients without OAuth, a Token is created by hand under **For developers** on Connected apps. It can be limited to reading, to chosen Areas and to an expiry date, and its value is shown once. The client sends it to the MCP endpoint, `/mcp` (Streamable HTTP), as `Authorization: Bearer <token>`. It finds one tool for every operation except the few that only make sense in a browser, such as signing in and managing Tokens. The same header works for the HTTP API, `POST /api/<operation>`.

## Develop

To contribute, read [CONTRIBUTING.md](CONTRIBUTING.md) first: what is welcome, signing off your commits, and when a change is done.

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
