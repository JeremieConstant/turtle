# Releasing Turtle

A Release of Turtle is a GitHub Release with handwritten notes, ready-built binaries and the container image `ghcr.io/turtle-wiki/turtle:X.Y.Z`. Turtle follows [SemVer](https://semver.org) and stays at 0.x until 1.0. Every Release below 1.0, and every one with a suffix such as `-rc.1`, is published as a pre-release; from 1.0 on a Release without a suffix becomes the latest.

The number comes from one place, `version` in `server/Cargo.toml`. Turtle shows it at the foot of the account menu, prints it with `turtle --version`, names it in the first line it logs and gives it to MCP clients. A build from a tag shows the plain number, such as `0.2.0`, and links its notes; any other build adds the commit it was built from, such as `0.2.0+3f9c2a1`, or `+dev` where no commit is known.

## Steps

1. Bump the version in `server/Cargo.toml`, `server/Cargo.lock` and `web/package.json`. `scripts/cargo check --manifest-path server/Cargo.toml` updates the lock file.
2. Write the notes in `docs/releases/vX.Y.Z.md` from the template below.
3. Merge both into `main` through a pull request.
4. Tag `main` and push the tag:

   ```bash
   git switch main && git pull && git tag vX.Y.Z && git push origin vX.Y.Z
   ```

5. Check what the tag published:
   - the Release, with the notes as its body, three archives and `SHA256SUMS`, under [Releases](https://github.com/turtle-wiki/turtle/releases)
   - that a downloaded archive passes `sha256sum -c SHA256SUMS --ignore-missing` and its binary prints `turtle X.Y.Z` with `--version`
   - the image `ghcr.io/turtle-wiki/turtle:X.Y.Z`, built by the Image workflow

`.github/workflows/release.yml` stops before building anything when the tag does not match the version in `server/Cargo.toml` and `web/package.json`, or when `docs/releases/vX.Y.Z.md` is missing. `scripts/check-release-tag.sh vX.Y.Z` runs the same check locally.

## The binaries

Each Release ships Turtle for three targets, each packed as `turtle-X.Y.Z-<target>.tar.gz` with `LICENSE` and the README:

- `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`, statically linked, so they run on any Linux without a matching glibc
- `aarch64-apple-darwin`

The macOS binary is not signed. macOS blocks it on its first start until the quarantine flag is removed:

```bash
xattr -d com.apple.quarantine turtle
```

## Template for the notes

Leave out a section that has nothing to say, except Updating.

```markdown
One or two sentences on what this Release is about.

## New

- **What a person can now do.** How it works, in a sentence or two.

## Changed

- What works differently than before.

## Fixed

- What was wrong and is right now.

## Updating

Say plainly whether Turtle changes the data directory when it starts, and
whether anything has to be done by hand, such as a new setting. Remind people
to keep a backup of the data directory before they update.

## Thanks

To [Name](https://github.com/login) for what they contributed.
```

Contributors from outside the project are named under Thanks, each with a link to their GitHub profile.
