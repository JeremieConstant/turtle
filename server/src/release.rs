//! Which Release of Turtle this is. The number comes from `version` in
//! `Cargo.toml`; `build.rs` says whether this is a release build and which
//! commit it was built from.

use schemars::JsonSchema;
use serde::Serialize;
use std::sync::LazyLock;

/// Where every Release's notes are published.
const RELEASES: &str = "https://github.com/turtle-wiki/turtle/releases/tag";

/// This Turtle's Release, as the app, the MCP server and the command line name it.
pub static RELEASE: LazyLock<Release> = LazyLock::new(|| {
    Release::of(
        env!("CARGO_PKG_VERSION"),
        option_env!("TURTLE_BUILT_FROM"),
        option_env!("TURTLE_BUILT_AS_RELEASE").is_some(),
    )
});

/// A Release of Turtle, as the account menu shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Release {
    /// `0.2.0` in a release build, `0.2.0+3f9c2a1` in any other, and
    /// `0.2.0+dev` when the commit is not known.
    pub number: String,
    /// The Release's notes on GitHub, in a release build only.
    pub notes: Option<String>,
}

impl Release {
    fn of(version: &str, commit: Option<&str>, release_build: bool) -> Release {
        if release_build {
            return Release {
                number: version.to_owned(),
                notes: Some(format!("{RELEASES}/v{version}")),
            };
        }
        let commit = commit.filter(|commit| !commit.is_empty()).unwrap_or("dev");
        Release {
            number: format!("{version}+{commit}"),
            notes: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Release;

    #[test]
    fn a_release_build_is_named_by_its_number_and_links_its_notes() {
        assert_eq!(
            Release::of("0.2.0", Some("3f9c2a1"), true),
            Release {
                number: "0.2.0".into(),
                notes: Some("https://github.com/turtle-wiki/turtle/releases/tag/v0.2.0".into()),
            }
        );
    }

    #[test]
    fn a_pre_release_links_the_notes_of_its_own_tag() {
        assert_eq!(
            Release::of("0.2.0-rc.1", None, true).notes.as_deref(),
            Some("https://github.com/turtle-wiki/turtle/releases/tag/v0.2.0-rc.1")
        );
    }

    #[test]
    fn any_other_build_names_the_commit_it_was_built_from_and_links_nothing() {
        assert_eq!(
            Release::of("0.2.0", Some("3f9c2a1"), false),
            Release { number: "0.2.0+3f9c2a1".into(), notes: None }
        );
    }

    #[test]
    fn without_a_known_commit_a_build_says_dev() {
        assert_eq!(Release::of("0.2.0", None, false).number, "0.2.0+dev");
        assert_eq!(Release::of("0.2.0", Some(""), false).number, "0.2.0+dev");
    }
}
