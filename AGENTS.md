## Checks

`scripts/check.sh` is the definition of done: it runs everything CI runs. The pre-commit hook runs it, so commit to run it. Coding standards for review are in `CODING_STANDARDS.md`.

Run cargo as `scripts/cargo`, which finds it off PATH. The server is a binary crate: run some of its unit tests with `scripts/cargo test --manifest-path server/Cargo.toml --bin turtle <name>` from the repository root.

## Trying it by hand

`scripts/try.sh` builds, runs and seeds a Turtle of its own on a free port with a fresh data directory, beside any other session's, and prints its address; it runs until stopped. Underneath, `scripts/preview.sh` builds and runs Turtle at `http://127.0.0.1:4100` and `scripts/seed.sh` gives a fresh one an account, Areas and Pages; both take `TURTLE_PORT` and `TURTLE_DATA`. To sign in from a script or the browser console, send `seed-account.json` from the data directory as it is to the `sign_in` operation (`POST /api/sign_in`). To try a file upload (an import, an Attachment) in the browser pane, follow `docs/agents/browser-uploads.md`.

## Commits

The commit that finishes an issue ends its message with `Closes #N`. A bare `(#N)` in the title does not close the issue on push. Commits for an unfinished issue use `Refs #N`. Every commit is signed off (`git commit -s`), or CI fails the pull request; see `CONTRIBUTING.md`.

## Agent skills

### Issue tracker

Issues and specs live in this repo's GitHub Issues. Before running `gh issue` or `gh pr`, read `docs/agents/issue-tracker.md`: `gh issue view <number> --json number,title,body,labels,comments` reads one.

### Triage labels

The five default triage labels are used unchanged: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
