#!/usr/bin/env bash
# Tests for check-sign-off.sh, each in a throwaway repository.
set -uo pipefail
check="$(cd "$(dirname "$0")" && pwd)/check-sign-off.sh"
failures=0

# The pre-commit hook runs this with git's own variables set, which would
# point the test repositories at the real one and its author.
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_PREFIX \
  GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_AUTHOR_DATE \
  GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_COMMITTER_DATE

# Creates an empty repository with one commit, changes into it, and sets
# $dir to it and $base to that commit.

repo() {
  dir=$(mktemp -d)
  cd "$dir"
  git init --quiet --initial-branch=main
  git config user.name "Ada Example"
  git config user.email "ada@example.com"
  git config commit.gpgsign false
  git commit --quiet --allow-empty -m "Base"
  base=$(git rev-parse HEAD)
}

commit() { git commit --quiet --allow-empty "$@"; }

expect() {
  local name=$1 want_status=$2 want_output=$3
  output=$("$check" "$base" HEAD 2>&1)
  status=$?
  if [ "$status" != "$want_status" ] || ! grep -qF -- "$want_output" <<< "$output"; then
    printf 'FAIL %s\n  status %s, wanted %s\n  output:\n%s\n' "$name" "$status" "$want_status" "$output"
    failures=$((failures + 1))
  else
    printf 'ok   %s\n' "$name"
  fi
}

repo
commit -m "Unsigned change"
expect "a commit without a sign-off fails and is named" 1 "$(git rev-parse --short HEAD) Unsigned change"

rm -rf "$dir"
repo
commit -s -m "First signed change"
commit -s -m "Second signed change"
expect "commits all signed off by their author pass" 0 "Every commit is signed off by its author."

rm -rf "$dir"
repo
commit -m "Signed by someone else" -m "Signed-off-by: Grace Other <grace@example.com>"
expect "a sign-off by someone other than the author fails" 1 "Signed by someone else"

rm -rf "$dir"
repo
commit -m "Unsigned change"
expect "a failure says how to sign off the last commit" 1 "git commit --amend -s"
expect "a failure says how to sign off every commit" 1 "git rebase --signoff"

rm -rf "$dir"
repo
git checkout --quiet -b feature
commit -s -m "Signed change on the branch"
git checkout --quiet main
commit -s -m "Signed change on main"
git checkout --quiet feature
git merge --quiet --no-edit main
expect "a merge commit without a sign-off passes" 0 "Every commit is signed off by its author."

rm -rf "$dir"
repo
commit --author "Bot <bot@example.com>" -m "Bot change"
SIGN_OFF_EXEMPT_AUTHOR="Bot <bot@example.com>" \
  expect "an unsigned commit by the exempt author passes" 0 "Every commit is signed off by its author."
expect "the same commit fails when no author is exempt" 1 "Bot change"

rm -rf "$dir"
repo
commit --author "Bot <bot@example.com>" -m "Bot change"
commit -m "Unsigned change pushed onto the bot's branch"
SIGN_OFF_EXEMPT_AUTHOR="Bot <bot@example.com>" \
  expect "another author's unsigned commit still fails beside the exempt one" 1 "Unsigned change pushed onto"

rm -rf "$dir"
repo
commit -s -m "Signed change"
base=no-such-branch
expect "a base that does not exist fails" 2 "no-such-branch"

rm -rf "$dir"
[ "$failures" = 0 ]
