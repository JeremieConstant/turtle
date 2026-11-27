//! Just enough HTTP/1.1 to call the server's API from tests: one request per
//! connection, the whole response read at once.

use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Sends one request and returns the response.
pub async fn request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Response {
    request_with_bytes(addr, method, path, headers, body.as_bytes()).await
}

/// Sends one request whose body is a file's bytes, and returns the response.
pub async fn request_with_bytes(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Response {
    tokio::time::timeout(TIMEOUT, async {
        let mut stream = TcpStream::connect(addr).await.expect("connecting to the server");
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Length: {}\r\n",
            body.len()
        );
        for (name, value) in headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes()).await.unwrap();
        // A server refusing a body that is too large answers before it has
        // all of it and closes, so writing the rest may fail; its answer is
        // read all the same.
        if let Err(error) = stream.write_all(body).await {
            assert!(cut_short(&error), "sending the request: {error}");
        }
        let mut raw = Vec::new();
        // On the heap: a buffer on the stack would make every request's future that large.
        let mut chunk = vec![0; 64 * 1024];
        loop {
            match stream.read(&mut chunk).await {
                Ok(0) => break,
                Ok(read) => raw.extend_from_slice(&chunk[..read]),
                Err(error) if cut_short(&error) && !raw.is_empty() => break,
                Err(error) => panic!("reading the response: {error}"),
            }
        }
        // An image's bytes that are not UTF-8 become U+FFFD; text is read as sent.
        parse(&String::from_utf8_lossy(&raw))
    })
    .await
    .unwrap_or_else(|_| panic!("{method} {path} timed out"))
}

/// Whether the server closed the connection while it was still in use.
fn cut_short(error: &std::io::Error) -> bool {
    matches!(error.kind(), std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset)
}

fn parse(raw: &str) -> Response {
    let (head, body) = raw.split_once("\r\n\r\n").expect("a response has a head");
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .expect("a response starts with its status");
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect();
    let chunked = headers
        .iter()
        .any(|(name, value)| name.eq_ignore_ascii_case("transfer-encoding") && value.contains("chunked"));
    let body = if chunked { unchunk(body) } else { body.to_string() };
    Response { status, headers, body }
}

fn unchunk(mut rest: &str) -> String {
    let mut body = String::new();
    while let Some((size, after)) = rest.split_once("\r\n") {
        let size = usize::from_str_radix(size.trim(), 16).expect("a chunk size");
        if size == 0 {
            break;
        }
        body.push_str(&after[..size]);
        rest = &after[size + 2..];
    }
    body
}
