#!/usr/bin/env bash
# Fails when a commit on <head> that is not on <base> lacks a Signed-off-by
# line naming its author, the sign-off under the Developer Certificate of
# Origin that CONTRIBUTING.md asks for. CI runs it on every pull request.
# Merge commits are left out: they bring in work, they do not add any.
#
#   scripts/check-sign-off.sh origin/main HEAD
set -euo pipefail

if [ $# -ne 2 ]; then
  echo "usage: $0 <base> <head>" >&2
  exit 2
fi
base=$1
head=$2

for ref in "$base" "$head"; do
  if ! git rev-parse --quiet --verify "$ref^{commit}" > /dev/null; then
    echo "$ref is not a commit in this repository." >&2
    exit 2
  fi
done

# Kept in a variable first: a failure inside the for loop's word list would
# not stop the script.
commits=$(git rev-list --no-merges "$base..$head")

unsigned=()
for commit in $commits; do
  author=$(git show -s --format='%an <%ae>' "$commit")
  sign_offs=$(git show -s --format='%(trailers:key=Signed-off-by,valueonly)' "$commit")
  if ! grep -qxF -- "$author" <<< "$sign_offs"; then
    unsigned+=("$(git show -s --format='%h %s (by %an <%ae>)' "$commit")")
  fi
done

if [ ${#unsigned[@]} -eq 0 ]; then
  echo "Every commit is signed off by its author."
  exit 0
fi

# The commit the branch started from exists in the contributor's clone too,
# whatever their remotes are called, and rebasing onto it rewrites only
# their own commits.
start=$(git merge-base "$base" "$head" | cut -c1-12)

cat <<EOF
These commits lack a "Signed-off-by:" line naming their author:

$(printf '  %s\n' "${unsigned[@]}")

Signing off confirms the Developer Certificate of Origin
(https://developercertificate.org); see CONTRIBUTING.md. To sign off:

  the last commit:     git commit --amend -s --no-edit
  every commit since:  git rebase --signoff $start

then push again with --force-with-lease.
EOF
exit 1
