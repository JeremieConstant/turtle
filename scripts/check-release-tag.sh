#!/usr/bin/env bash
# Checks that a tag vX.Y.Z may become a Release, before anything is built:
# the version in server/Cargo.toml and web/package.json is X.Y.Z, and its
# notes are in docs/releases/vX.Y.Z.md. Run it from the repository root,
# with the tag as its argument. Prints the number and whether it is a
# pre-release as `number=` and `prerelease=` lines, for GITHUB_OUTPUT.
set -euo pipefail

tag=${1:?usage: check-release-tag.sh vX.Y.Z}
fail() { echo "error: $*" >&2; exit 1; }

[[ $tag =~ ^v([0-9]+)\.([0-9]+)\.([0-9]+)(-[0-9A-Za-z.-]+)?$ ]] ||
  fail "the tag $tag is not a Release number such as v0.2.0 or v0.2.0-rc.1"
number=${tag#v}
major=${BASH_REMATCH[1]}
suffix=${BASH_REMATCH[4]}

# The first `version = "..."` in Cargo.toml is the package's.
cargo_version=$(sed -n 's/^version = "\(.*\)"$/\1/p' server/Cargo.toml | head -n 1)
[ "$cargo_version" = "$number" ] ||
  fail "the tag $tag does not match version \"$cargo_version\" in server/Cargo.toml"

web_version=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' web/package.json | head -n 1)
[ "$web_version" = "$number" ] ||
  fail "the tag $tag does not match version \"$web_version\" in web/package.json"

notes=docs/releases/$tag.md
[ -s "$notes" ] || fail "the Release has no notes: $notes is missing or empty"

# Below 1.0, and with a suffix such as -rc.1, a Release is a pre-release.
if [ "$major" = 0 ] || [ -n "$suffix" ]; then prerelease=true; else prerelease=false; fi
echo "number=$number"
echo "prerelease=$prerelease"
