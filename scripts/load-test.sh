#!/usr/bin/env bash
# Runs the load test (server/tests/load.rs): by default 500 people editing at
# the same time on 100 Pages for a minute, against a release build. It is not
# part of scripts/check.sh. Change the load with TURTLE_LOAD_EDITORS,
# TURTLE_LOAD_PAGES, TURTLE_LOAD_SECONDS and TURTLE_LOAD_KEYSTROKE_MS.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/lib/find-cargo.sh

# Every editor holds a connection, an open file on both ends; the default
# limit (256 on macOS) is far too low.
ulimit -n 10240 2>/dev/null || ulimit -n "$(ulimit -Hn)"

# The server is measured as it ships, a release build, which embeds the web app.
if [ ! -f web/dist/index.html ]; then
  npm --prefix web ci
  npm --prefix web run build
fi

exec cargo test --release --quiet --manifest-path server/Cargo.toml --test load -- --ignored --nocapture
