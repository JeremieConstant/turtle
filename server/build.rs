//! Embeds the built web app (`web/dist`) into the binary, so a release needs
//! no files beside it. Writes the list of files that `src/web_app.rs` includes.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

fn main() -> io::Result<()> {
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
