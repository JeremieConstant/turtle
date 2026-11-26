#!/usr/bin/env bash
# Tests for check-release-tag.sh, each in a throwaway directory laid out
# like the repository.
set -uo pipefail
check="$(cd "$(dirname "$0")" && pwd)/check-release-tag.sh"
failures=0

# A repository at `version` with notes for `notes` (none if empty).
repo() {
  local version=$1 notes=${2-}
  dir=$(mktemp -d)
  mkdir -p "$dir/server" "$dir/web" "$dir/docs/releases"
  printf '[package]\nname = "turtle"\nversion = "%s"\n\n[dependencies]\nanyhow = { version = "1.0" }\n' "$version" > "$dir/server/Cargo.toml"
  printf '{\n  "name": "turtle-web",\n  "version": "%s",\n  "private": true\n}\n' "$version" > "$dir/web/package.json"
  if [ -n "$notes" ]; then echo "What is new." > "$dir/docs/releases/$notes.md"; fi
  cd "$dir"
}

expect() {
  local name=$1 tag=$2 want_status=$3 want_output=$4
  output=$("$check" "$tag" 2>&1)
  status=$?
  if [ "$status" != "$want_status" ] || ! grep -qF -- "$want_output" <<< "$output"; then
    printf 'FAIL %s\n  status %s, wanted %s\n  output:\n%s\n' "$name" "$status" "$want_status" "$output"
    failures=$((failures + 1))
  else
    printf 'ok   %s\n' "$name"
  fi
  cd / && rm -rf "$dir"
}

repo 0.2.0 v0.2.0
expect "a tag matching both versions, with notes, passes" v0.2.0 0 "number=0.2.0"
repo 0.2.0 v0.2.0
expect "a Release below 1.0 is a pre-release" v0.2.0 0 "prerelease=true"
repo 1.0.0 v1.0.0
expect "from 1.0 on a Release without a suffix is not a pre-release" v1.0.0 0 "prerelease=false"
repo 1.1.0-rc.1 v1.1.0-rc.1
expect "a Release with a suffix is a pre-release" v1.1.0-rc.1 0 "prerelease=true"
repo 0.2.0 v0.2.1
expect "a tag that does not match Cargo.toml fails and names it" v0.2.1 1 "server/Cargo.toml"
repo 0.2.0 v0.2.0
sed -i.bak 's/"version": "0.2.0"/"version": "0.1.0"/' web/package.json
expect "a tag that does not match package.json fails and names it" v0.2.0 1 "web/package.json"
repo 0.2.0
expect "a tag without notes fails and names the missing file" v0.2.0 1 "docs/releases/v0.2.0.md"
repo 0.2.0 v0.2.0
expect "a tag that is not a Release number fails" release-1 1 "not a Release number"

if [ "$failures" -gt 0 ]; then
  printf '\n%s test(s) failed.\n' "$failures"
  exit 1
fi
