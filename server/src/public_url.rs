//! The address Turtle is reached at from outside, `TURTLE_PUBLIC_URL`.
//! OAuth is offered only when it is known (ADR-0010).

use std::fmt;

/// The http or https address of Turtle's root. Turtle is served at the root
/// of its host, so the address has no path; it is kept without a trailing
/// slash, so every address under it is the public URL followed by its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicUrl {
    address: String,
    host: String,
}

impl PublicUrl {
    pub fn parse(value: &str) -> anyhow::Result<PublicUrl> {
        let address = value.trim().trim_end_matches('/');
        let host = address
            .strip_prefix("https://")
            .or_else(|| address.strip_prefix("http://"))
            .filter(|rest| !rest.contains(['/', '?', '#', '@']) && !rest.chars().any(char::is_whitespace))
            .and_then(host_of);
        let Some(host) = host else {
            anyhow::bail!(
                "TURTLE_PUBLIC_URL must be the http or https address Turtle is reached at, such as https://turtle.example.org, not {value:?}"
            );
        };
        Ok(PublicUrl { address: address.to_string(), host })
    }

    pub fn as_str(&self) -> &str {
        &self.address
    }

    /// The host, lowercased, without port, brackets or the dot that may end
    /// a fully qualified name.
    pub fn host(&self) -> &str {
        &self.host
    }
}

impl fmt::Display for PublicUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.address)
    }
}

/// The host of the part of an address after its scheme.
fn host_of(rest: &str) -> Option<String> {
    let host = match rest.strip_prefix('[') {
        Some(bracketed) => bracketed.split_once(']')?.0,
        None => rest.split(':').next()?,
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_public_url_is_an_http_address_without_a_trailing_slash() {
        assert_eq!(PublicUrl::parse("https://turtle.example.org/").unwrap().as_str(), "https://turtle.example.org");
        assert_eq!(PublicUrl::parse(" http://localhost:4100 ").unwrap().as_str(), "http://localhost:4100");
        for invalid in [
            "turtle.example.org",
            "ftp://example.org",
            "https://",
            "https://example.org/turtle",
            "https://example.org/?a=1",
            "https://example.org/#x",
            "https://user@example.org",
        ] {
            assert!(PublicUrl::parse(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn its_host_is_lowercased_without_port_brackets_or_final_dot() {
        for (url, host) in [
            ("https://Turtle.Example.org:8443", "turtle.example.org"),
            ("http://turtle.LAN.", "turtle.lan"),
            ("http://[::1]:4100", "::1"),
            ("http://127.0.0.1", "127.0.0.1"),
        ] {
            assert_eq!(PublicUrl::parse(url).unwrap().host(), host, "{url}");
        }
    }
}
