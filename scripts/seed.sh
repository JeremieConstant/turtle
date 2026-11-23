#!/usr/bin/env bash
# Fills a fresh Turtle with something to look at: the first account, two
# Areas, and a small tree in one of them. Run it once Turtle is listening
# (scripts/preview.sh), with the same TURTLE_PORT and TURTLE_DATA. It only
# works on an installation without an account, so it never touches real
# content.
#
# The account's password is made up anew on every run and written only to a
# file in the data directory that nobody else on the machine can read, so no
# installation ever has a password that can be looked up in this repository.
set -euo pipefail
cd "$(dirname "$0")/.."

base="http://127.0.0.1:${TURTLE_PORT:-4100}"
data="${TURTLE_DATA:-turtle-data-dev}"
account="$data/seed-account.txt"
email="ada@example.test"
password=$(head -c 24 /dev/urandom | base64 | tr -d '/+=')
cookies=$(mktemp)
trap 'rm -f "$cookies"' EXIT

# Calls an operation and prints its answer; stops with the operation's message if it failed.
op() {
  local answer
  answer=$(curl --silent --show-error --cookie "$cookies" --cookie-jar "$cookies" \
    --header 'content-type: application/json' --data "$2" "$base/api/$1")
  if [ "$(jq 'has("error")' <<<"$answer")" = true ]; then
    echo "$1 failed: $(jq -r '.error.message' <<<"$answer")" >&2
    exit 1
  fi
  printf '%s' "$answer"
}

if [ "$(op get_session '{}' | jq '.setup_required')" != true ]; then
  echo "Turtle at $base already has an account, so there is nothing to seed. Use a fresh TURTLE_DATA directory." >&2
  exit 1
fi

if [ ! -d "$data" ]; then
  echo "There is no data directory at $data. Run with the TURTLE_DATA that Turtle was started with." >&2
  exit 1
fi
(umask 077 && printf 'Email: %s\nPassword: %s\n' "$email" "$password" >"$account")
op create_first_account "$(jq -n --arg email "$email" --arg password "$password" \
  '{email: $email, name: "Ada Example", password: $password}')" >/dev/null

platform=$(op create_area '{"name": "Platform"}' | jq -r '.id')
op create_area '{"name": "Handbook"}' >/dev/null

# Creates a Page or Folder and prints its ID. $1: page or folder, $2: title, $3: the ID to put it beneath, if any.
entry() {
  op "create_$1" "$(jq -n --arg area "$platform" --arg title "$2" --arg parent "${3:-}" \
    '{area_id: $area, title: $title} + (if $parent == "" then {} else {parent_id: $parent} end)')" | jq -r '.id'
}

runbooks=$(entry folder "Runbooks")
deploying=$(entry page "Deploying to production" "$runbooks")
entry page "Rollback" "$deploying" >/dev/null
glossary=$(entry page "Glossary")

echo "Seeded $base. The email and password to sign in with are in $account."
echo "A Page to open once signed in: $base/p/$glossary"
