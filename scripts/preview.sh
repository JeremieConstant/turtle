#!/usr/bin/env bash
# Runs Turtle for trying it out by hand, on its own data directory beside
# the repository. The web app is built first: the server binary embeds
# whatever web/dist holds when it is compiled, so without this it serves
# the web app as it was at the last build.
#
# Turtle listens on http://127.0.0.1:${TURTLE_PORT:-4100}, not on localhost.
# Once it runs, scripts/seed.sh fills a fresh data directory with an account,
# Areas and Pages to look at.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/lib/find-cargo.sh

# Only what the page needs: fresh operation types and the bundle. The type
# check is left to scripts/check.sh, so it never holds up trying something.
npm --prefix web run --silent generate
(cd web && npx vite build --logLevel warn)
TURTLE_DATA="${TURTLE_DATA:-turtle-data-dev}" exec cargo run --manifest-path server/Cargo.toml
