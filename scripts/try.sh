#!/usr/bin/env bash
# Builds, runs and seeds a Turtle of its own, to try something by hand
# beside any other Turtle on this machine: it takes a free port and a fresh
# data directory, unless TURTLE_PORT or TURTLE_DATA say otherwise, starts
# Turtle with scripts/preview.sh, fills it with scripts/seed.sh, and says
# where it is. It keeps running until it is stopped, and stops Turtle with it.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -z "${TURTLE_PORT:-}" ]; then
  TURTLE_PORT=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
fi
tmp="${TMPDIR:-/tmp}"
TURTLE_DATA="${TURTLE_DATA:-$(mktemp -d "${tmp%/}/turtle-try.XXXXXX")}"
export TURTLE_PORT TURTLE_DATA
base="http://127.0.0.1:$TURTLE_PORT"

scripts/preview.sh &
turtle=$!
trap 'kill "$turtle" 2>/dev/null || true' EXIT

# The first build takes minutes; a Turtle that stops while building ends the wait.
until curl --silent --output /dev/null "$base/"; do
  if ! kill -0 "$turtle" 2>/dev/null; then
    echo "Turtle stopped before it listened on $base." >&2
    exit 1
  fi
  sleep 2
done

scripts/seed.sh
echo "Turtle runs at $base with its data in $TURTLE_DATA. Stop this script to stop it."
wait "$turtle"
