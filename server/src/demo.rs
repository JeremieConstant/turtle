//! Demo mode: a Turtle that says it is a Demo, how often it is reset, and
//! which demo Member anyone may sign in as. Resetting is up to the hosting.

/// What `TURTLE_DEMO_EMAIL`, `TURTLE_DEMO_PASSWORD` and
/// `TURTLE_DEMO_RESET_HOURS` say.
#[derive(Debug)]
pub struct DemoSettings {
    pub email: String,
    pub password: String,
    pub resets_every_hours: Option<u32>,
}

/// Reads the demo settings. Demo mode is on when the email and password are
/// both set; setting only one, or an interval without them, is a mistake.
/// A setting of nothing but spaces counts as unset.
pub fn from_settings(
    email: Option<&str>,
    password: Option<&str>,
    reset_hours: Option<&str>,
) -> anyhow::Result<Option<DemoSettings>> {
    let email = email.map(str::trim).filter(|email| !email.is_empty());
    // Kept as written: spaces around a password may be part of it.
    let password = password.filter(|password| !password.trim().is_empty());
    let reset_hours = reset_hours.map(str::trim).filter(|hours| !hours.is_empty());
    let (email, password) = match (email, password) {
        (Some(email), Some(password)) => (email, password),
        (Some(_), None) => anyhow::bail!("TURTLE_DEMO_PASSWORD must be set along with TURTLE_DEMO_EMAIL: the demo Member's password"),
        (None, Some(_)) => anyhow::bail!("TURTLE_DEMO_EMAIL must be set along with TURTLE_DEMO_PASSWORD: the demo Member's email address"),
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
    }))
}

/// Tells the operator why the demo Member is not shown. `Core` makes sure
/// this happens once per start.
pub fn warn_about_workspace_admin() {
    eprintln!(
        "TURTLE_DEMO_EMAIL belongs to a Workspace Admin. Turtle will not publish a Workspace Admin's password, so the demo Member is not shown. Use a Member without Workspace Admin."
    );
}
