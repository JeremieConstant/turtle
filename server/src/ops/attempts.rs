//! Failed attempts at signing in and at using single-use links. Whoever
//! fails too often in a short time is turned away for a while, before their
//! attempt costs a slow password hash, so nobody can guess endlessly or keep
//! the server busy hashing.
//!
//! Attempts are counted in memory: a restart forgets them, which costs a
//! guesser no more than a short wait would.

use super::{Client, Core, OpError};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;

/// How long a failed attempt counts.
const WINDOW: i64 = 15 * 60;

/// Failures with one email address within the window before it is refused.
const EMAIL_LIMIT: usize = 5;

/// Failures from one client within the window before it is refused. Higher
/// than for an email address, since people in one office, or everyone when
/// a proxy hides their addresses, share one.
const CLIENT_LIMIT: usize = 20;

/// Above this many counted things, those whose failures have all aged out
/// are dropped, so guessing with ever new addresses cannot fill the memory.
/// Once that leaves many, the next round waits until there are twice as
/// many, so it never runs on every attempt.
const PRUNE_ABOVE: usize = 10_000;

/// What failed attempts are counted against.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Counted {
    /// A normalized email address, whether or not a Member has it, so a
    /// refusal does not tell which addresses have an account.
    Email(String),
    /// The address a request came from. An IPv6 client is counted by its
    /// /64 network, as anyone with one address has the whole network.
    Client(IpAddr),
}

impl Counted {
    fn client(address: IpAddr) -> Counted {
        let address = match address {
            IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
                Some(v4) => IpAddr::V4(v4),
                None => IpAddr::V6((u128::from(v6) & !((1u128 << 64) - 1)).into()),
            },
            v4 => v4,
        };
        Counted::Client(address)
    }

    fn limit(&self) -> usize {
        match self {
            Counted::Email(_) => EMAIL_LIMIT,
            Counted::Client(_) => CLIENT_LIMIT,
        }
    }
}

/// The failed attempts of the last window, by what they count against.
#[derive(Default)]
pub struct Attempts {
    counts: Mutex<Counts>,
}

#[derive(Default)]
struct Counts {
    failed: HashMap<Counted, Vec<i64>>,
    /// How many counted things there may be before the next pruning.
    prune_above: usize,
}

/// An attempt under way. It counts as failed from the start, so attempts
/// made at the same time cannot all slip under the limit, and stops
/// counting once it `succeeded`.
#[must_use]
pub struct Attempt<'a> {
    attempts: &'a Attempts,
    at: i64,
    counted: Vec<Counted>,
}

impl Attempt<'_> {
    /// A successful attempt is no failure.
    pub fn succeeded(self) {
        let failed = &mut self.attempts.counts.lock().unwrap().failed;
        for counted in &self.counted {
            if let Some(times) = failed.get_mut(counted) {
                if let Some(position) = times.iter().position(|time| *time == self.at) {
                    times.remove(position);
                }
            }
        }
    }
}

/// Starts an attempt from `client`, at signing in with `email` or, without
/// one, at using a single-use link. Refused, before anything slow happens,
/// while the client or the email address has failed too often lately.
pub fn begin<'a>(core: &'a Core, client: Client, email: Option<&str>) -> Result<Attempt<'a>, OpError> {
    let counted: Vec<Counted> = email
        .map(|email| Counted::Email(super::accounts::normalize_email(email)))
        .into_iter()
        .chain(client.map(Counted::client))
        .collect();
    let attempts = core.attempts();
    let now = core.now();
    let mut counts = attempts.counts.lock().unwrap();
    if counts.failed.len() > counts.prune_above.max(PRUNE_ABOVE) {
        counts.failed.retain(|_, times| times.iter().any(|time| *time > now - WINDOW));
        counts.prune_above = counts.failed.len() * 2;
    }
    let failed = &mut counts.failed;
    // When every one over its limit has had enough failures age out.
    let mut free_at = None;
    for counted in &counted {
        let times = failed.entry(counted.clone()).or_default();
        times.retain(|time| *time > now - WINDOW);
        if times.len() >= counted.limit() {
            let frees = times[times.len() - counted.limit()] + WINDOW;
            free_at = free_at.max(Some(frees));
        }
    }
    if let Some(free_at) = free_at {
        return Err(too_many(free_at - now));
    }
    for counted in &counted {
        failed.get_mut(counted).expect("entered above").push(now);
    }
    Ok(Attempt {
        attempts,
        at: now,
        counted,
    })
}

fn too_many(seconds: i64) -> OpError {
    let minutes = (seconds + 59) / 60;
    let unit = if minutes == 1 { "minute" } else { "minutes" };
    OpError::NotPermitted(format!("Too many attempts failed. Try again in {minutes} {unit}."))
}
