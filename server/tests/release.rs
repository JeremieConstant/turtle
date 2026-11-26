//! Which Release a Turtle is, as the command line and its log say.

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// What a test build calls itself: never a release build, so the number and
/// the commit it was built from, or `dev`.
fn names_a_test_build(number: &str) -> bool {
    number
        .strip_prefix(concat!(env!("CARGO_PKG_VERSION"), "+"))
        .is_some_and(|commit| !commit.is_empty() && !commit.contains(char::is_whitespace))
}

#[tokio::test]
async fn turtle_version_prints_the_release_and_leaves_the_data_directory_alone() {
    let parent = tempfile::tempdir().unwrap();
    let data_dir = parent.path().join("data");
    let output = Command::new(env!("CARGO_BIN_EXE_turtle"))
        .arg("--version")
        .env("TURTLE_DATA", &data_dir)
        .output()
        .await
        .expect("running turtle --version");

    assert!(output.status.success());
    let printed = String::from_utf8(output.stdout).unwrap();
    let number = printed.trim_end().strip_prefix("turtle ").unwrap_or_else(|| panic!("{printed:?}"));
    assert!(names_a_test_build(number), "{printed:?}");
    assert!(!data_dir.exists(), "--version created the data directory");
}

#[tokio::test]
async fn the_first_line_turtle_logs_names_its_release() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut process = Command::new(env!("CARGO_BIN_EXE_turtle"))
        .env("TURTLE_DATA", data_dir.path())
        .env("TURTLE_PORT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("starting turtle");

    let first = BufReader::new(process.stdout.take().unwrap())
        .lines()
        .next_line()
        .await
        .unwrap()
        .expect("turtle logged nothing");
    let number = first
        .strip_prefix("Starting Turtle ")
        .and_then(|rest| rest.strip_suffix('.'))
        .unwrap_or_else(|| panic!("{first:?}"));
    assert!(names_a_test_build(number), "{first:?}");
}
