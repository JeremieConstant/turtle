#!/usr/bin/env bash
# Every check CI runs, in one command: lints, the types generated from the
# operation catalogue, the web app's type check and tests, and the server's
# tests. Run it from anywhere in the repository before committing.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n== %s\n' "$1"; }

step "Server lints"
cargo clippy --quiet --manifest-path server/Cargo.toml --all-targets -- -D warnings

step "Generated operation types match the catalogue"
# Compared with the file as it was, not with git, so work not yet staged
# passes once the file is regenerated.
generated=web/src/lib/operations.gen.ts
before=$(shasum "$generated" 2>/dev/null || true)
npm --prefix web run --silent generate
if [ "$before" != "$(shasum "$generated")" ]; then
  echo "$generated was out of date and has been regenerated. Run the checks again, and commit it." >&2
  exit 1
fi

step "Web app type check"
npm --prefix web run --silent check

step "Web app tests"
npm --prefix web test --silent

step "Server tests"
cargo test --quiet --manifest-path server/Cargo.toml

printf '\nAll checks passed.\n'
