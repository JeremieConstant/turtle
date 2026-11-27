#!/usr/bin/env bash
# Imports a Confluence space export into a fresh Turtle, as a release build,
# and prints how long each stage took and how much memory Turtle held by
# then: to try a large space, a real one or one from
# scripts/make-confluence-space.py. It runs the ignored test
# import_a_real_export and is not part of scripts/check.sh.
#
#   scripts/measure-import.sh space.zip
#
# Peak memory comes from /usr/bin/time where it can tell it. A real export
# holds people's data: keep it outside the repository.
set -euo pipefail
if [ $# -ne 1 ] || [ ! -f "$1" ]; then
  echo "usage: scripts/measure-import.sh <export.zip>" >&2
  exit 1
fi
export CONFLUENCE_EXPORT
CONFLUENCE_EXPORT=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
cd "$(dirname "$0")/.."
source scripts/lib/find-cargo.sh

# Built first and run as the test binary itself, so the measurement holds
# only the import, not cargo.
binary=$(cargo test --release --quiet --manifest-path server/Cargo.toml --bin turtle --no-run --message-format=json \
  | python3 -I -c 'import json, sys
for line in sys.stdin:
    built = json.loads(line)
    if built.get("reason") == "compiler-artifact" and built["profile"]["test"] and built["target"]["name"] == "turtle" and built.get("executable"):
        print(built["executable"])')
test=("$binary" import_a_real_export --ignored --nocapture)
case "$(uname)" in
  Darwin) /usr/bin/time -l "${test[@]}" 2> >(grep -E "maximum resident set size" >&2) ;;
  Linux) /usr/bin/time -v "${test[@]}" 2> >(grep -E "Maximum resident set size" >&2) ;;
  *) "${test[@]}" ;;
esac
