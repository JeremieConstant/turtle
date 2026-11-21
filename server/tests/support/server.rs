//! Runs the real `turtle` binary on a free port with its own data directory.

use super::http;
use super::TestClient;
use serde_json::Value;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

const START_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_TIMEOUT: Duration = Duration::from_secs(10);

pub struct TestServer {
    /// Killed on drop if a test ends without `stop` (when it fails, say).
    process: Option<Child>,
    addr: SocketAddr,
    data_dir: TempDir,
}

impl TestServer {
    /// Starts a server with an empty data directory of its own.
    pub async fn start() -> TestServer {
        let data_dir = tempfile::tempdir().expect("creating a temporary data directory");
        let (process, addr) = launch(data_dir.path()).await;
        TestServer {
            process: Some(process),
            addr,
            data_dir,
        }
    }

    /// Where the server listens, for clients that connect without `connect`
    /// (the load test).
    #[allow(dead_code)]
    pub fn addr(&self) -> SocketAddr {
        self.addr
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

    /// Stops the server and starts it again on the same data directory. The
    /// port may change.
    pub async fn restart(&mut self) {
        if self.process.is_some() {
            self.stop().await;
        }
        let (process, addr) = launch(self.data_dir.path()).await;
        self.process = Some(process);
        self.addr = addr;
    }
}

async fn launch(data_dir: &Path) -> (Child, SocketAddr) {
    let mut process = Command::new(env!("CARGO_BIN_EXE_turtle"))
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
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });

    (process, SocketAddr::from(([127, 0, 0, 1], port)))
}

/// The port from "Turtle is listening on http://127.0.0.1:<port> ...".
fn announced_port(line: &str) -> Option<u16> {
    let rest = line.split_once("listening on http://127.0.0.1:")?.1;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|&port| port != 0)
}

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
