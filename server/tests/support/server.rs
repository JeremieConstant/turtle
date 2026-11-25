//! Runs the real `turtle` binary on a free port with its own data directory.

use super::http;
use super::TestClient;
use serde_json::Value;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
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
    settings: Settings,
    /// Every line the server has written to its standard error.
    logged: Arc<Mutex<Vec<String>>>,
}

/// The settings a test may give the server; the rest are left unset.
#[derive(Debug, Clone, Default)]
struct Settings {
    /// `TURTLE_MAX_ATTACHMENT_MB`.
    attachment_limit_mb: Option<u32>,
    /// `TURTLE_PUBLIC_URL`.
    public_url: Option<String>,
    /// `TURTLE_HOST`.
    host: Option<String>,
    /// `TURTLE_CLIENT_ADDRESS_HEADER`.
    client_address_header: Option<String>,
    demo: DemoEnv,
}

/// `TURTLE_DEMO_EMAIL`, `TURTLE_DEMO_PASSWORD` and `TURTLE_DEMO_RESET_HOURS`,
/// each when the test sets it. Values are passed as written, so a test can
/// give an invalid one.
#[derive(Debug, Clone, Default)]
pub struct DemoEnv {
    pub email: Option<String>,
    pub password: Option<String>,
    pub reset_hours: Option<String>,
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
        TestServer::start_with(None, Settings::default()).await
    }

    /// Starts a server that says it is reachable at `url`, so it offers OAuth.
    pub async fn start_with_public_url(url: &str) -> TestServer {
        TestServer::start_with(None, Settings { public_url: Some(url.to_string()), ..Settings::default() }).await
    }

    /// Starts a server that takes files of at most this many MB.
    pub async fn start_with_attachment_limit(mb: u32) -> TestServer {
        TestServer::start_with(None, Settings { attachment_limit_mb: Some(mb), ..Settings::default() }).await
    }

    /// Starts a server that reports each save it makes; `next_save` and
    /// `take_saves` return them.
    pub async fn start_reporting_saves() -> TestServer {
        TestServer::start_with(Some(Arc::new(Mutex::new(Vec::new()))), Settings::default()).await
    }

    /// Starts a server that listens on `host` (`TURTLE_HOST`).
    pub async fn start_listening_on(host: &str) -> TestServer {
        TestServer::start_with(None, Settings { host: Some(host.to_string()), ..Settings::default() }).await
    }

    /// Starts a server with `TURTLE_HOST` set to `host`, expecting it to
    /// refuse to start, and returns what it wrote to its standard error.
    pub async fn refuses_to_start_listening_on(host: &str) -> String {
        TestServer::refuses_to_start(Settings { host: Some(host.to_string()), ..Settings::default() }).await
    }

    /// Starts a server that takes the client's address from this header,
    /// as one behind a reverse proxy would (`TURTLE_CLIENT_ADDRESS_HEADER`).
    pub async fn start_with_client_address_header(name: &str) -> TestServer {
        let settings = Settings { client_address_header: Some(name.to_string()), ..Settings::default() };
        TestServer::start_with(None, settings).await
    }

    /// Starts a server with these demo settings.
    pub async fn start_as_demo(demo: DemoEnv) -> TestServer {
        TestServer::start_with(None, Settings { demo, ..Settings::default() }).await
    }

    /// Starts a server with these demo settings, expecting it to refuse to
    /// start, and returns what it wrote to its standard error.
    pub async fn refuses_to_start_as_demo(demo: DemoEnv) -> String {
        TestServer::refuses_to_start(Settings { demo, ..Settings::default() }).await
    }

    async fn refuses_to_start(settings: Settings) -> String {
        let data_dir = tempfile::tempdir().expect("creating a temporary data directory");
        let output = tokio::time::timeout(START_TIMEOUT, command(data_dir.path(), None, &settings).output())
            .await
            .expect("the server neither started nor stopped within the timeout")
            .expect("running the turtle binary");
        assert!(!output.status.success(), "the server started: {}", String::from_utf8_lossy(&output.stdout));
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    async fn start_with(saves: Option<Arc<Mutex<Vec<Save>>>>, settings: Settings) -> TestServer {
        let data_dir = tempfile::tempdir().expect("creating a temporary data directory");
        let logged = Arc::new(Mutex::new(Vec::new()));
        let (process, addr) = launch(data_dir.path(), saves.clone(), &settings, logged.clone()).await;
        TestServer {
            process: Some(process),
            addr,
            data_dir,
            saves,
            settings,
            logged,
        }
    }

    /// Every line the server has written to its standard error so far.
    pub async fn logged(&self) -> Vec<String> {
        self.logged.lock().await.clone()
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
        let (process, addr) = launch(self.data_dir.path(), self.saves.clone(), &self.settings, self.logged.clone()).await;
        self.process = Some(process);
        self.addr = addr;
    }
}

/// The command that runs the server on `data_dir` with exactly these settings.
fn command(data_dir: &Path, saves: Option<&Arc<Mutex<Vec<Save>>>>, settings: &Settings) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_turtle"));
    let given = [
        ("TURTLE_PUBLIC_URL", settings.public_url.clone()),
        ("TURTLE_HOST", settings.host.clone()),
        ("TURTLE_CLIENT_ADDRESS_HEADER", settings.client_address_header.clone()),
        ("TURTLE_MAX_ATTACHMENT_MB", settings.attachment_limit_mb.map(|mb| mb.to_string())),
        ("TURTLE_DEMO_EMAIL", settings.demo.email.clone()),
        ("TURTLE_DEMO_PASSWORD", settings.demo.password.clone()),
        ("TURTLE_DEMO_RESET_HOURS", settings.demo.reset_hours.clone()),
        ("TURTLE_LOG_SAVES", saves.map(|_| "1".to_string())),
    ];
    for (name, value) in given {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    command
        .env("TURTLE_DATA", data_dir)
        // Port 0: the system picks a free one, and the server says which.
        .env("TURTLE_PORT", "0")
        .env_remove("PORT")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

async fn launch(
    data_dir: &Path,
    saves: Option<Arc<Mutex<Vec<Save>>>>,
    settings: &Settings,
    logged: Arc<Mutex<Vec<String>>>,
) -> (Child, SocketAddr) {
    let mut process = command(data_dir, saves.as_ref(), settings)
        .spawn()
        .expect("starting the turtle binary");

    // Passed on, so a failing test still shows what the server said, and kept.
    let mut errors = BufReader::new(process.stderr.take().unwrap()).lines();
    tokio::spawn(async move {
        while let Ok(Some(line)) = errors.next_line().await {
            eprintln!("{line}");
            logged.lock().await.push(line);
        }
    });

    let stdout = process.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();
    let addr = tokio::time::timeout(START_TIMEOUT, async {
        while let Some(line) = lines.next_line().await.expect("reading the server's output") {
            if let Some(addr) = announced_addr(&line) {
                return addr;
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

    (process, addr)
}

/// Where to reach the server that said "Turtle is listening on
/// http://<address>:<port> ...": that address, or loopback when it listens
/// on every address.
fn announced_addr(line: &str) -> Option<SocketAddr> {
    let rest = line.split_once("listening on http://")?.1;
    let mut addr: SocketAddr = rest.split_whitespace().next()?.parse().ok()?;
    if addr.port() == 0 {
        return None;
    }
    if addr.ip().is_unspecified() {
        addr.set_ip(match addr {
            SocketAddr::V4(_) => Ipv4Addr::LOCALHOST.into(),
            SocketAddr::V6(_) => Ipv6Addr::LOCALHOST.into(),
        });
    }
    Some(addr)
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
