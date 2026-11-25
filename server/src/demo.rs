//! Demo mode: a Turtle that says it is a demo, how often it is reset, and
//! which account visitors may sign in with. Resetting is up to the hosting.

use std::sync::atomic::{AtomicBool, Ordering};

/// What `TURTLE_DEMO_EMAIL`, `TURTLE_DEMO_PASSWORD` and
/// `TURTLE_DEMO_RESET_HOURS` say.
#[derive(Debug)]
pub struct DemoSettings {
    pub email: String,
    pub password: String,
    pub resets_every_hours: Option<u32>,
    /// Whether the operator has been told, this start, that a Workspace
    /// Admin's password is not published.
    warned: AtomicBool,
}

impl DemoSettings {
    /// Logs, once per start, that the demo account is a Workspace Admin and
    /// so is not shown.
    pub fn warn_about_workspace_admin(&self) {
        if !self.warned.swap(true, Ordering::Relaxed) {
            eprintln!(
                "TURTLE_DEMO_EMAIL belongs to a Workspace Admin. Turtle will not publish a Workspace Admin's password, so the demo account is not shown. Use a Member without Workspace Admin."
            );
        }
    }
}

/// Reads the demo settings. Demo mode is on when the email and password are
/// both set; setting only one, or an interval without them, is a mistake.
pub fn from_settings(
    email: Option<&str>,
    password: Option<&str>,
    reset_hours: Option<&str>,
) -> anyhow::Result<Option<DemoSettings>> {
    let email = email.map(str::trim).filter(|email| !email.is_empty());
    let password = password.filter(|password| !password.is_empty());
    let reset_hours = reset_hours.map(str::trim).filter(|hours| !hours.is_empty());
    let (email, password) = match (email, password) {
        (Some(email), Some(password)) => (email, password),
        (Some(_), None) => anyhow::bail!("TURTLE_DEMO_PASSWORD must be set along with TURTLE_DEMO_EMAIL: the demo account's password"),
        (None, Some(_)) => anyhow::bail!("TURTLE_DEMO_EMAIL must be set along with TURTLE_DEMO_PASSWORD: the demo account's email address"),
        (None, None) => match reset_hours {
            Some(_) => anyhow::bail!("TURTLE_DEMO_RESET_HOURS must be set only in demo mode, along with TURTLE_DEMO_EMAIL and TURTLE_DEMO_PASSWORD"),
            None => return Ok(None),
        },
    };
    let resets_every_hours = match reset_hours {
        None => None,
        Some(hours) => match hours.parse::<u32>() {
            Ok(hours) if hours > 0 => Some(hours),
            _ => anyhow::bail!("TURTLE_DEMO_RESET_HOURS must be a whole number of hours above 0, not {hours:?}"),
        },
    };
    Ok(Some(DemoSettings {
        email: email.to_string(),
        password: password.to_string(),
        resets_every_hours,
        warned: AtomicBool::new(false),
    }))
}
