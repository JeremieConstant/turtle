## Checks

`scripts/check.sh` is the definition of done: it runs everything CI runs. The pre-commit hook runs it, so commit to run it. Coding standards for review are in `CODING_STANDARDS.md`.

Run cargo as `scripts/cargo`, which finds it off PATH. The server is a binary crate: run some of its unit tests with `../scripts/cargo test --bin turtle <name>` from `server/`.

## Trying it by hand

`scripts/preview.sh` builds and runs Turtle at `http://127.0.0.1:4100`; `scripts/seed.sh` then gives a fresh one an account, Areas and Pages, and prints a Page's address. Both take `TURTLE_PORT` and `TURTLE_DATA`, so a second Turtle can run beside another session's. To sign in from a script or the browser console, call the `sign_in` operation (`POST /api/sign_in`) with the email and password from `seed-account.txt` in the data directory.

## Commits

The commit that finishes an issue ends its message with `Closes #N`. A bare `(#N)` in the title does not close the issue on push. Commits for an unfinished issue use `Refs #N`.

## Agent skills

### Issue tracker

Issues and specs live in this repo's GitHub Issues, via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default triage labels are used unchanged: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
