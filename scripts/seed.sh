#!/usr/bin/env bash
# Fills a fresh Turtle with something to look at: the first account, two
# Areas, and a small tree with content in one of them. Run it once Turtle is listening
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
# The same, as the input of the sign_in operation, to send as it is.
sign_in="$data/seed-account.json"
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
(umask 077 && printf 'Email: %s\nPassword: %s\n' "$email" "$password" >"$account" \
  && jq -n --arg email "$email" --arg password "$password" '{email: $email, password: $password}' >"$sign_in")
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
rollback=$(entry page "Rollback" "$deploying")
glossary=$(entry page "Glossary")

# Content with each thing a Page can hold that reaches beyond its text: Page
# links, an image Attachment, a Tag and a table, so features that carry them
# along, such as export, can be tried by hand.
write() {
  op replace_page "$(jq -n --arg page "$1" --arg markdown "$2" '{page_id: $page, markdown: $markdown}')" >/dev/null
}
# A one-pixel PNG.
pixel=iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=
diagram=$(op attach_file "$(jq -n --arg page "$deploying" --arg content "$pixel" \
  '{page_id: $page, name: "pipeline.png", content: $content}')" | jq -r '.url')
write "$glossary" "$(printf '| Term | Meaning |\n| --- | --- |\n| Rollback | Going back to the last release |\n| Canary | A release to a few first |')"
write "$deploying" "$(printf '## Steps\n\n1. Check the terms in the [](turtle:%s).\n2. Release a canary.\n3. If it fails, follow [](turtle:%s).\n\n![The pipeline](%s)' "$glossary" "$rollback" "$diagram")"
write "$rollback" "$(printf 'Go back to [](turtle:%s) and release the previous version.' "$deploying")"
op add_tag "$(jq -n --arg page "$deploying" '{page_id: $page, tag: "ops"}')" >/dev/null

echo "Seeded $base. The email and password to sign in with are in $account."
echo "To sign in from a script or the browser console, send $sign_in as it is to POST /api/sign_in."
echo "The Pages, Folder and Attachment are in the Area Platform; the Area Handbook is empty."
echo "A Page to open once signed in: $base/p/$glossary"
