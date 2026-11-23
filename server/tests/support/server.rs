//! Runs the real `turtle` binary on a free port with its own data directory.

use super::http;
use super::TestClient;
use serde_json::Value;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

const START_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_TIMEOUT: Duration = Duration::from_secs(10);

pub struct TestServer {
    /// Killed on drop if a test ends without `stop` (when it fails, say).
    process: Option<Child>,
    addr: SocketAddr,
    data_dir: TempDir,
    /// When the server reports each save (`TURTLE_LOG_SAVES`): the saves
    /// it reported that nobody has taken yet.
    saves: Option<Arc<Mutex<Vec<Save>>>>,
    /// `TURTLE_MAX_ATTACHMENT_MB`, when the test sets it.
    attachment_limit_mb: Option<u32>,
    /// `TURTLE_PUBLIC_URL`, when the test sets it.
    public_url: Option<String>,
}

/// One round of saving changed Pages, as the server reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Save {
    /// How many Pages were written.
    pub pages: usize,
    pub took: Duration,
}

impl TestServer {
    /// Starts a server with an empty data directory of its own.
    pub async fn start() -> TestServer {
        TestServer::start_with(None, None, None).await
    }

    /// Starts a server that says it is reachable at `url`, so it offers OAuth.
    pub async fn start_with_public_url(url: &str) -> TestServer {
        TestServer::start_with(None, None, Some(url.to_string())).await
    }

    /// Starts a server that takes files of at most this many MB.
    pub async fn start_with_attachment_limit(mb: u32) -> TestServer {
        TestServer::start_with(None, Some(mb), None).await
    }

    /// Starts a server that reports each save it makes; `next_save` and
    /// `take_saves` return them.
    pub async fn start_reporting_saves() -> TestServer {
        TestServer::start_with(Some(Arc::new(Mutex::new(Vec::new()))), None, None).await
    }

    async fn start_with(
        saves: Option<Arc<Mutex<Vec<Save>>>>,
        attachment_limit_mb: Option<u32>,
        public_url: Option<String>,
    ) -> TestServer {
        let data_dir = tempfile::tempdir().expect("creating a temporary data directory");
        let (process, addr) = launch(data_dir.path(), saves.clone(), attachment_limit_mb, public_url.as_deref()).await;
        TestServer {
            process: Some(process),
            addr,
            data_dir,
            saves,
            attachment_limit_mb,
            public_url,
        }
    }

    fn reported_saves(&self) -> &Mutex<Vec<Save>> {
        self.saves.as_ref().expect("the server was not started reporting saves")
    }

    /// Where the server listens, for clients that connect without `connect`.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Waits for the next save the server reports. Panics with `what` if
    /// none comes within a few save intervals.
    pub async fn next_save(&self, what: &str) -> Save {
        let saves = self.reported_saves();
        let deadline = tokio::time::Instant::now() + SAVE_WAIT_TIMEOUT;
        loop {
            {
                let mut saves = saves.lock().await;
                if !saves.is_empty() {
                    return saves.remove(0);
                }
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("timed out waiting until {what}");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Every save reported since the last call.
    pub async fn take_saves(&self) -> Vec<Save> {
        std::mem::take(&mut *self.reported_saves().lock().await)
    }

    /// Something that reads the memory the server process holds at that
    /// moment (its resident set), for sampling while the server works.
    pub fn memory_reader(&self) -> impl Fn() -> u64 + Send + 'static {
        let pid = self
            .process
            .as_ref()
            .and_then(Child::id)
            .expect("the server is not running");
        move || {
            let output = std::process::Command::new("ps")
                .args(["-o", "rss=", "-p", &pid.to_string()])
                .output()
                .expect("running ps");
            let kilobytes: u64 = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse()
                .expect("ps reports the resident set in kilobytes");
            kilobytes * 1024
        }
    }

    /// Opens a Page the way the editor does, as the signed-in Member, and
    /// waits for its content.
    pub async fn connect(&self, member: &Member, page: &str) -> TestClient {
        TestClient::connect(self.addr, page, Some(&member.cookie)).await
    }

    /// Creates the first account of the installation and signs it in.
    pub async fn sign_up(&self) -> Member {
        let response = http::request(
            self.addr,
            "POST",
            "/api/create_first_account",
            &[("Content-Type", "application/json")],
            &serde_json::json!({ "name": "Ada", "email": EMAIL, "password": PASSWORD }).to_string(),
        )
        .await;
        assert_eq!(response.status, 200, "signing up: {}", response.body);
        self.signed_in(response)
    }

    /// Signs the Member from `sign_up` in once more, in a session of its own.
    pub async fn sign_in(&self) -> Member {
        let response = http::request(
            self.addr,
            "POST",
            "/api/sign_in",
            &[("Content-Type", "application/json")],
            &serde_json::json!({ "email": EMAIL, "password": PASSWORD }).to_string(),
        )
        .await;
        assert_eq!(response.status, 200, "signing in: {}", response.body);
        self.signed_in(response)
    }

    /// Has `admin`, a Workspace Admin, invite a new Member, who accepts the
    /// invitation and is signed in.
    pub async fn invite(&self, admin: &Member, name: &str) -> Member {
        let email = format!("{}@example.org", name.to_lowercase());
        let link = admin.call("invite_member", serde_json::json!({ "email": email })).await;
        let address = link["address"].as_str().expect("an invitation has an address");
        let secret = address.rsplit('/').next().unwrap();
        let response = http::request(
            self.addr,
            "POST",
            "/api/accept_invitation",
            &[("Content-Type", "application/json")],
            &serde_json::json!({ "secret": secret, "name": name, "password": PASSWORD }).to_string(),
        )
        .await;
        assert_eq!(response.status, 200, "accepting an invitation: {}", response.body);
        self.signed_in(response)
    }

    fn signed_in(&self, response: http::Response) -> Member {
        let cookie = response
            .header("set-cookie")
            .and_then(|value| value.split(';').next())
            .expect("signing in sets the session cookie")
            .to_string();
        Member { addr: self.addr, cookie }
    }

    /// Stops the server the way an operator does and waits until it has exited.
    /// Panics if it does not exit, or exits with a failure.
    pub async fn stop(&mut self) {
        let mut process = self.process.take().expect("the server is not running");
        let pid = process.id().expect("the server has already exited");
        let signalled = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .expect("running kill");
        assert!(signalled.success(), "could not signal the server");
        let status = tokio::time::timeout(STOP_TIMEOUT, process.wait())
            .await
            .expect("the server did not stop within the timeout")
            .expect("waiting for the server");
        assert!(status.success(), "the server stopped with {status}");
    }

    /// The data directory the server keeps everything in.
    pub fn data_dir(&self) -> &Path {
        self.data_dir.path()
    }

    /// Stops the server and starts it again on the same data directory. The
    /// port may change.
    pub async fn restart(&mut self) {
        if self.process.is_some() {
            self.stop().await;
        }
        let (process, addr) = launch(
            self.data_dir.path(),
            self.saves.clone(),
            self.attachment_limit_mb,
            self.public_url.as_deref(),
        )
        .await;
        self.process = Some(process);
        self.addr = addr;
    }
}

async fn launch(
    data_dir: &Path,
    saves: Option<Arc<Mutex<Vec<Save>>>>,
    attachment_limit_mb: Option<u32>,
    public_url: Option<&str>,
) -> (Child, SocketAddr) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_turtle"));
    match public_url {
        Some(url) => command.env("TURTLE_PUBLIC_URL", url),
        None => command.env_remove("TURTLE_PUBLIC_URL"),
    };
    match attachment_limit_mb {
        Some(mb) => command.env("TURTLE_MAX_ATTACHMENT_MB", mb.to_string()),
        None => command.env_remove("TURTLE_MAX_ATTACHMENT_MB"),
    };
    if saves.is_some() {
        command.env("TURTLE_LOG_SAVES", "1");
    } else {
        command.env_remove("TURTLE_LOG_SAVES");
    }
    let mut process = command
        .env("TURTLE_DATA", data_dir)
        // Port 0: the system picks a free one, and the server says which.
        .env("TURTLE_PORT", "0")
        .env_remove("PORT")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .expect("starting the turtle binary");

    let stdout = process.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();
    let port = tokio::time::timeout(START_TIMEOUT, async {
        while let Some(line) = lines.next_line().await.expect("reading the server's output") {
            if let Some(port) = announced_port(&line) {
                return port;
            }
        }
        panic!("the server exited before it was listening");
    })
    .await
    .expect("the server did not start within the timeout");

    // Keep reading, so the server never blocks on a full pipe.
    tokio::spawn(async move {
        while let Ok(Some(line)) = lines.next_line().await {
            if let (Some(saves), Some(save)) = (&saves, reported_save(&line)) {
                saves.lock().await.push(save);
            }
        }
    });

    (process, SocketAddr::from(([127, 0, 0, 1], port)))
}

/// The port from "Turtle is listening on http://127.0.0.1:<port> ...".
fn announced_port(line: &str) -> Option<u16> {
    let rest = line.split_once("listening on http://127.0.0.1:")?.1;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|&port| port != 0)
}

/// A save from "Saved 3 Pages in 1.234 ms." (or "1 Page"), the line
/// `server/src/main.rs` prints; the live-sync test notices if they part.
fn reported_save(line: &str) -> Option<Save> {
    let rest = line.strip_prefix("Saved ")?;
    let (pages, rest) = rest.split_once(' ')?;
    let millis = rest.split_once(" in ")?.1.strip_suffix(" ms.")?;
    Some(Save {
        pages: pages.parse().ok()?,
        took: Duration::from_secs_f64(millis.parse::<f64>().ok()? / 1000.0),
    })
}

const SAVE_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

const EMAIL: &str = "ada@example.org";
const PASSWORD: &str = "correct horse battery";

/// A signed-in Member, calling operations as the web app does.
pub struct Member {
    addr: SocketAddr,
    /// `name=value` of the session cookie.
    pub cookie: String,
}

impl Member {
    /// Calls an operation and returns its output. Panics if it fails.
    pub async fn call(&self, operation: &str, input: Value) -> Value {
        let response = http::request(
            self.addr,
            "POST",
            &format!("/api/{operation}"),
            &[("Content-Type", "application/json"), ("Cookie", &self.cookie)],
            &input.to_string(),
        )
        .await;
        assert_eq!(response.status, 200, "{operation}: {}", response.body);
        serde_json::from_str(&response.body).expect("an operation answers with JSON")
    }

    /// The Member's ID.
    pub async fn id(&self) -> String {
        let session = self.call("get_session", serde_json::json!({})).await;
        session["member"]["id"].as_str().expect("signed in").to_string()
    }

    /// Calls an operation and returns the HTTP status and the body.
    pub async fn try_call(&self, operation: &str, input: Value) -> (u16, String) {
        let response = http::request(
            self.addr,
            "POST",
            &format!("/api/{operation}"),
            &[("Content-Type", "application/json"), ("Cookie", &self.cookie)],
            &input.to_string(),
        )
        .await;
        (response.status, response.body)
    }

    /// Creates an Area with a Page at its top, and returns the Page's ID.
    pub async fn create_page(&self, title: &str) -> String {
        let area = self.call("create_area", serde_json::json!({ "name": "Engineering" })).await;
        let page = self
            .call("create_page", serde_json::json!({ "area_id": area["id"], "title": title }))
            .await;
        page["id"].as_str().expect("a Page has an ID").to_string()
    }
}
