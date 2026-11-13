#!/usr/bin/env bash
# Check a clean commit with the shared local gate, then publish its PR.
set -euo pipefail
branch=$(git rev-parse --abbrev-ref HEAD)
[ "$branch" != main ] && [ "$branch" != HEAD ] || { echo "Land a named branch, not main or detached HEAD."; exit 2; }
clean() {
  local status
  status=$(git status --porcelain --untracked-files=all) || return 1
  [ -z "$status" ]
}
clean || { echo "A clean HEAD is required: commit or remove outstanding work, then rerun."; exit 2; }
sha=$(git rev-parse HEAD)
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$(git rev-parse --show-toplevel)/target}
git fetch -q origin main
git merge-tree --write-tree origin/main "$sha" >/dev/null ||
  { echo "Merge conflict with main: merge origin/main, resolve it, commit, and rerun."; exit 1; }

title=$(git show -s --format=%s "$sha")
node scripts/pr-lint.mjs --title "$title" --branch "$branch"
node scripts/check.mjs
unchanged() {
  [ "$(git rev-parse HEAD)" = "$sha" ] && [ "$(git rev-parse --abbrev-ref HEAD)" = "$branch" ] && clean ||
    { echo "HEAD or working files changed during landing. Rerun on a clean commit."; return 1; }
}
unchanged
echo "Checked SHA: $sha"
body=$(mktemp)
trap 'rm -f -- "$body"' EXIT
git show -s --format=%b "$sha" > "$body"
if ! grep -q '^## Handoff' "$body"; then
  printf '\n## Handoff\n\nLeft: CI and squash merge. Next: `gh pr checks --watch`. Blockers: none.\n' >> "$body"
fi
printf '\nLocal gate passed for `%s`.\n' "$sha" >> "$body"
actor=$(gh api user --jq .login)
[ "$actor" = diyanko ] || { echo "Landing requires the maintainer's GitHub account."; exit 1; }
pr=$(gh pr list --head "$branch" --base main --state open --json number,author,isCrossRepository --jq '.[0] | if . == null then empty else [.number, .author.login, .isCrossRepository] | @tsv end')
if [ -n "$pr" ]; then
  read -r number author fork <<< "$pr"
  [ "$author" = "$actor" ] && [ "$fork" = false ] || { echo "Only the maintainer's own in-repository PR can auto-merge."; exit 1; }
fi
unchanged
git push -q -u origin "$sha:refs/heads/$branch"
if [ -n "$pr" ]; then
  gh pr edit "$number" --title "$title" --body-file "$body"
  url=$(gh pr view "$number" --json url --jq .url)
else
  url=$(gh pr create --base main --head "$branch" --title "$title" --body-file "$body")
fi
gh pr merge "$url" --squash --auto --match-head-commit "$sha"
printf '%s\nNext: gh pr checks %s --watch; after merge, check main CI before nightly publication.\n' "$url" "$url"
