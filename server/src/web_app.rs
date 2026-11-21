//! The web app, served from inside the binary so Turtle needs no files
//! beside it. `build.rs` embeds whatever `web/dist` holds at compile time.

use axum::extract::State;
use axum::http::{header, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Router;

/// A file of the web app: its address without the leading slash, and its content.
pub type File = (&'static str, &'static [u8]);

/// The web app as it was built when this binary was compiled.
pub const EMBEDDED: &[File] = include!(concat!(env!("OUT_DIR"), "/web_app_files.rs"));

/// Addresses below these never belong to a screen of the web app, so an
/// unknown one is an error instead of the app.
const NOT_SCREENS: &[&str] = &["api", "ws", "assets"];

/// Answers every address no other route claims: a file of the web app with
/// that file, and any other address the app could show with the app itself,
/// so reloading a Page address works (ADR-0007).
pub fn router(files: &'static [File]) -> Router {
    Router::new().fallback(serve).with_state(files)
}

async fn serve(State(files): State<&'static [File]>, method: Method, uri: Uri) -> Response {
    if method != Method::GET && method != Method::HEAD {
        return StatusCode::NOT_FOUND.into_response();
    }
    let name = uri.path().trim_start_matches('/');
    if let Some(file) = find(files, name) {
        return file_response(file);
    }
    let first = name.split('/').next().unwrap_or_default();
    if NOT_SCREENS.contains(&first) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match find(files, "index.html") {
        Some(app) => file_response(app),
        None => (
            StatusCode::NOT_FOUND,
            "This Turtle was built without the web app. Build web/ and then the server again.",
        )
            .into_response(),
    }
}

fn find(files: &'static [File], name: &str) -> Option<File> {
    files.iter().copied().find(|(file, _)| *file == name)
}

fn file_response((name, content): File) -> Response {
    // Vite puts a content hash into every name under assets/, so those can be
    // kept for good. Everything else is checked for changes on each visit.
    let cache = if name.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        [
            (header::CONTENT_TYPE, content_type(name)),
            (header::CACHE_CONTROL, cache),
        ],
        content,
    )
        .into_response()
}

fn content_type(name: &str) -> &'static str {
    match name.rsplit_once('.').map_or("", |(_, extension)| extension) {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "webmanifest" => "application/manifest+json",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{header, Method, Request, StatusCode};
    use tower::ServiceExt;

    const FILES: &[File] = &[
        ("index.html", b"<!doctype html><title>Turtle</title>"),
        ("assets/app-3f9a.js", b"console.log('turtle')"),
        ("favicon.svg", b"<svg/>"),
    ];

    struct Answer {
        status: StatusCode,
        content_type: Option<String>,
        cache_control: Option<String>,
        body: String,
    }

    async fn request(files: &'static [File], method: Method, address: &str) -> Answer {
        let request = Request::builder()
            .method(method)
            .uri(address)
            .body(Body::empty())
            .unwrap();
        let response = router(files).oneshot(request).await.unwrap();
        let header = |name| {
            response
                .headers()
                .get(name)
                .map(|value: &axum::http::HeaderValue| value.to_str().unwrap().to_string())
        };
        Answer {
            status: response.status(),
            content_type: header(header::CONTENT_TYPE),
            cache_control: header(header::CACHE_CONTROL),
            body: String::from_utf8(
                axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap()
                    .to_vec(),
            )
            .unwrap(),
        }
    }

    async fn get(address: &str) -> Answer {
        request(FILES, Method::GET, address).await
    }

    #[tokio::test]
    async fn the_start_address_is_answered_with_the_app() {
        let answer = get("/").await;
        assert_eq!(answer.status, StatusCode::OK);
        assert_eq!(
            answer.content_type.as_deref(),
            Some("text/html; charset=utf-8")
        );
        assert_eq!(answer.body, "<!doctype html><title>Turtle</title>");
    }

    #[tokio::test]
    async fn a_file_of_the_app_is_served_with_its_type() {
        let script = get("/assets/app-3f9a.js").await;
        assert_eq!(script.status, StatusCode::OK);
        assert_eq!(
            script.content_type.as_deref(),
            Some("text/javascript; charset=utf-8")
        );
        assert_eq!(script.body, "console.log('turtle')");

        let icon = get("/favicon.svg").await;
        assert_eq!(icon.content_type.as_deref(), Some("image/svg+xml"));
    }

    #[tokio::test]
    async fn reloading_a_page_address_is_answered_with_the_app() {
        for address in [
            "/p/8f2c/release-notes",
            "/p/8f2c",
            "/sign-in",
            "/setup?next=%2F",
        ] {
            let answer = get(address).await;
            assert_eq!(answer.status, StatusCode::OK, "{address}");
            assert_eq!(
                answer.body, "<!doctype html><title>Turtle</title>",
                "{address}"
            );
        }
    }

    #[tokio::test]
    async fn unknown_addresses_outside_the_app_are_not_found() {
        for address in [
            "/api",
            "/api/pages/1/unknown",
            "/ws",
            "/ws/a/b",
            "/assets/gone-11aa.js",
        ] {
            let answer = get(address).await;
            assert_eq!(answer.status, StatusCode::NOT_FOUND, "{address}");
            assert_ne!(
                answer.body, "<!doctype html><title>Turtle</title>",
                "{address}"
            );
        }
    }

    #[tokio::test]
    async fn only_reading_is_answered_with_the_app() {
        let answer = request(FILES, Method::POST, "/p/8f2c").await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn the_app_is_checked_for_changes_while_its_hashed_files_are_kept() {
        assert_eq!(
            get("/p/8f2c").await.cache_control.as_deref(),
            Some("no-cache")
        );
        assert_eq!(
            get("/assets/app-3f9a.js").await.cache_control.as_deref(),
            Some("public, max-age=31536000, immutable")
        );
    }

    #[tokio::test]
    async fn a_build_without_the_web_app_says_so() {
        let answer = request(&[], Method::GET, "/").await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND);
        assert!(
            answer.body.contains("built without the web app"),
            "{}",
            answer.body
        );
    }
}
