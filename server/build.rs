//! Embeds the built web app (`web/dist`) into the binary, so a release needs
//! no files beside it. Writes the list of files that `src/web_app.rs` includes,
//! and tells `src/release.rs` how this build is named.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs, io};

fn main() -> io::Result<()> {
    name_the_release();

    let dist = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../web/dist");
    println!("cargo::rerun-if-changed={}", dist.display());

    let mut files = Vec::new();
    if dist.is_dir() {
        collect(&dist, &dist, &mut files)?;
    }
    files.sort();

    // A debug build may go without the web app: generating the web app's
    // types needs the server first. A release must not ship without it.
    let has_app = files.iter().any(|(name, _)| name == "index.html");
    if !has_app && env::var("PROFILE").as_deref() == Ok("release") {
        eprintln!(
            "error: {} holds no built web app. Run `npm ci && npm run build` in web/ first.",
            dist.display()
        );
        std::process::exit(1);
    }

    let mut list = String::from("&[\n");
    for (name, path) in &files {
        writeln!(list, "    ({name:?}, include_bytes!({path:?})),").unwrap();
    }
    list.push(']');
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("web_app_files.rs"),
        list,
    )
}

/// A build is a release build when `TURTLE_RELEASE_BUILD` is `true`, as the
/// release workflow and the image of a tag set it. Any other build is named
/// by its commit: `TURTLE_COMMIT` where there is no `.git`, as in the
/// container build, or else the one git has checked out.
fn name_the_release() {
    println!("cargo::rerun-if-env-changed=TURTLE_RELEASE_BUILD");
    println!("cargo::rerun-if-env-changed=TURTLE_COMMIT");
    if env::var("TURTLE_RELEASE_BUILD").as_deref() == Ok("true") {
        println!("cargo::rustc-env=TURTLE_BUILT_AS_RELEASE=1");
    }
    let commit = match env::var("TURTLE_COMMIT") {
        Ok(commit) if !commit.trim().is_empty() => Some(commit.trim().to_owned()),
        _ => checked_out_commit(),
    };
    if let Some(commit) = commit {
        let short: String = commit.chars().take(7).collect();
        println!("cargo::rustc-env=TURTLE_BUILT_FROM={short}");
    }
}

/// The commit git has checked out, if this is a clone. The files that change
/// with it are watched, so a new commit renames the build.
fn checked_out_commit() -> Option<String> {
    let git = |args: &[&str]| {
        let output = Command::new("git").args(args).output().ok()?;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let commit = git(&["rev-parse", "--short=7", "HEAD"])?;
    let mut watched = vec![git(&["rev-parse", "--git-path", "HEAD"])?];
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watched.push(git(&["rev-parse", "--git-path", &branch])?);
    }
    watched.push(git(&["rev-parse", "--git-path", "packed-refs"])?);
    for path in watched {
        // Watching a file that does not exist would rebuild every time.
        if Path::new(&path).exists() {
            println!("cargo::rerun-if-changed={path}");
        }
    }
    Some(commit)
}

/// Every file below `dir`, with its address relative to `root`.
fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, files)?;
            continue;
        }
        let name = path
            .strip_prefix(root)
            .unwrap()
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let path = path.canonicalize()?.to_string_lossy().into_owned();
        files.push((name, path));
    }
    Ok(())
}
