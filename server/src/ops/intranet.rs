//! Whether Turtle looks like it runs only in an intranet, where claude.ai,
//! which reaches a connector from the internet, could not reach it.
//!
//! Turtle decides this from its own public address alone and never calls out
//! to test whether it is reachable: at most it asks the system resolver what
//! its host name stands for, as any program on the machine would.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::sync::Arc;

/// Finds the addresses a host name stands for. Answers none when it cannot
/// tell.
pub type Resolver = Arc<dyn Fn(&str) -> Vec<IpAddr> + Send + Sync>;

/// Asks the system resolver, like any lookup on this machine.
pub fn system_resolver() -> Resolver {
    Arc::new(|host| match (host, 0).to_socket_addrs() {
        Ok(found) => found.map(|address| address.ip()).collect(),
        Err(_) => Vec::new(),
    })
}

/// Host name endings that only mean something inside a network.
const INSIDE_ENDINGS: [&str; 4] = [".local", ".internal", ".lan", ".localhost"];

/// Whether Turtle, reached at `public_url`, looks reachable only from inside
/// its network: it has no public URL, or its host is `localhost`, a private or
/// loopback address, a name ending in `.local`, `.internal` or `.lan`, or a
/// name that resolves only to private addresses. A name that resolves to
/// nothing is given the benefit of the doubt, since this machine may simply
/// have no resolver.
pub fn looks_intranet_only(public_url: Option<&str>, resolve: &Resolver) -> bool {
    let Some(host) = public_url.and_then(host_of) else { return true };
    if let Ok(address) = host.parse::<IpAddr>() {
        return is_private(address);
    }
    if host == "localhost" || INSIDE_ENDINGS.iter().any(|ending| host.ends_with(ending)) {
        return true;
    }
    let addresses = resolve(&host);
    !addresses.is_empty() && addresses.into_iter().all(is_private)
}

/// The host of a public URL, lowercased, without port, brackets or the dot
/// that may end a fully qualified name.
fn host_of(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let host = match rest.strip_prefix('[') {
        Some(bracketed) => bracketed.split_once(']')?.0,
        None => rest.split(':').next()?,
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Whether an address can be reached only from inside a network.
fn is_private(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => is_private_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_private_v4(v4),
            None => is_private_v6(v6),
        },
    }
}

fn is_private_v4(address: Ipv4Addr) -> bool {
    let [first, second, ..] = address.octets();
    // 100.64.0.0/10 is shared by carrier-grade NAT, out of the internet's reach.
    let shared = first == 100 && (64..128).contains(&second);
    address.is_private() || address.is_loopback() || address.is_link_local() || address.is_unspecified() || shared
}

fn is_private_v6(address: Ipv6Addr) -> bool {
    let first = address.segments()[0];
    let unique_local = first & 0xfe00 == 0xfc00;
    let link_local = first & 0xffc0 == 0xfe80;
    address.is_loopback() || address.is_unspecified() || unique_local || link_local
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolving_to(addresses: &'static [&'static str]) -> Resolver {
        Arc::new(move |_| addresses.iter().map(|address| address.parse().unwrap()).collect())
    }

    /// A resolver for addresses that are no names, which must not be asked.
    fn never_asked() -> Resolver {
        Arc::new(|host| panic!("{host} was looked up"))
    }

    #[test]
    fn without_a_public_url_turtle_looks_intranet_only() {
        assert!(looks_intranet_only(None, &never_asked()));
    }

    #[test]
    fn localhost_looks_intranet_only() {
        assert!(looks_intranet_only(Some("http://localhost:4100"), &never_asked()));
        assert!(looks_intranet_only(Some("http://LocalHost"), &never_asked()));
        assert!(looks_intranet_only(Some("http://turtle.localhost"), &never_asked()));
    }

    #[test]
    fn a_private_or_loopback_address_looks_intranet_only() {
        for url in [
            "http://127.0.0.1:4100",
            "http://10.1.2.3",
            "http://172.16.0.9",
            "https://192.168.178.20:8443",
            "http://169.254.1.1",
            "http://100.64.0.1",
            "http://[::1]:4100",
            "http://[fd12:3456:789a::1]",
            "http://[fe80::1]",
            "http://[::ffff:192.168.0.1]",
        ] {
            assert!(looks_intranet_only(Some(url), &never_asked()), "{url}");
        }
    }

    #[test]
    fn a_public_address_does_not() {
        assert!(!looks_intranet_only(Some("http://93.184.215.14"), &never_asked()));
        assert!(!looks_intranet_only(Some("https://[2606:2800:21f:cb07:6820:80da:af6b:8b2c]"), &never_asked()));
    }

    #[test]
    fn a_host_ending_in_an_inside_name_looks_intranet_only() {
        for url in ["http://turtle.local", "https://wiki.corp.internal", "http://turtle.lan:4100", "http://turtle.LAN."] {
            assert!(looks_intranet_only(Some(url), &never_asked()), "{url}");
        }
    }

    #[test]
    fn a_host_resolving_only_to_private_addresses_looks_intranet_only() {
        let private = resolving_to(&["10.0.0.5", "fd00::5"]);
        assert!(looks_intranet_only(Some("https://turtle.example.org"), &private));
    }

    #[test]
    fn a_host_resolving_to_a_public_address_does_not() {
        let public = resolving_to(&["93.184.215.14"]);
        assert!(!looks_intranet_only(Some("https://turtle.example.org"), &public));
        let mixed = resolving_to(&["10.0.0.5", "2606:2800:21f:cb07:6820:80da:af6b:8b2c"]);
        assert!(!looks_intranet_only(Some("https://turtle.example.org"), &mixed));
    }

    #[test]
    fn a_host_resolving_to_nothing_is_given_the_benefit_of_the_doubt() {
        assert!(!looks_intranet_only(Some("https://turtle.example.org"), &resolving_to(&[])));
    }

    #[test]
    fn the_host_alone_is_looked_up() {
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = asked.clone();
        let resolve: Resolver = Arc::new(move |host| {
            recorder.lock().unwrap().push(host.to_string());
            vec!["93.184.215.14".parse().unwrap()]
        });
        looks_intranet_only(Some("https://Turtle.Example.org:8443"), &resolve);
        assert_eq!(*asked.lock().unwrap(), ["turtle.example.org"]);
    }
}
