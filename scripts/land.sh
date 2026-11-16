#!/usr/bin/env bash
# Fast local feedback; hosted CI result decides the squash merge (no merge queue).
set -euo pipefail
if [ "${1:-}" = start ] || [ "${1:-}" = end ]; then
  action=$1 name=${2:-} issue=${3:-}
  if [ "$#" -lt 2 ] || { [ "$action" = start ] && [ "$#" -gt 3 ]; } || { [ "$action" = end ] && [ "$#" -ne 2 ]; }; then
    echo 'Usage: land.sh start NAME [ISSUE] | end NAME'; exit 2
  fi
  [[ "$name" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]] && [ "$name" != main ] || { echo 'NAME must be lowercase letters or digits separated by hyphens, and not main.'; exit 2; }
  [[ -z "$issue" || "$issue" =~ ^[0-9]+$ ]] || { echo 'ISSUE must be a number.'; exit 2; }
  root=$(git rev-parse --show-toplevel)
  worktree="$root/../ibis-work/$name"
  if [ "$action" = start ]; then
    title="chore: ${name//-/ }"
    node "$root/scripts/pr-lint.mjs" --title "$title" --branch "$name"
    git fetch -q origin main
    git worktree add -b "$name" "$worktree" origin/main
    (cd "$worktree"; pnpm -C apps/desktop install --frozen-lockfile --offline; git commit --allow-empty -s -m "$title"; git push -u origin "$name")
    body='Handoff: implement the change; next run the focused check; blockers: none.'
    [ -z "$issue" ] || body=$(printf 'Refs #%s\n\n%s' "$issue" "$body")
    gh pr create --draft --base main --head "$name" --title "$title" --body "$body"
  else
    [ -d "$worktree" ] || { echo 'Worktree not found.'; exit 1; }
    status=$(git -C "$worktree" status --porcelain --untracked-files=all)
    [ -z "$status" ] || { echo 'Worktree has uncommitted changes.'; exit 1; }
    current=$(git -C "$worktree" rev-parse --abbrev-ref HEAD)
    head=$(git -C "$worktree" rev-parse HEAD)
    if [ "$current" = "$name" ]; then
      filter=".[] | select(.headRefOid == \"$head\") | .headRefOid"
    elif [ "$current" = main ] && ! git show-ref --verify --quiet "refs/heads/$name"; then
      # gh --delete-branch may already have checked out main and deleted the branch.
      filter='.[0].headRefOid // empty'
    else
      echo 'Worktree branch does not match NAME.'; exit 1
    fi
    [ -n "$(gh pr list --head "$name" --base main --state merged --json headRefOid --jq "$filter")" ] || { echo 'No merged PR for this worktree head.'; exit 1; }
    git worktree remove "$worktree"
    if git show-ref --verify --quiet "refs/heads/$name"; then git branch -D "$name"; fi
  fi
  exit
fi
title_arg='' body_file=''
while [ "$#" -gt 0 ]; do
  case "$1" in
    --title|--body-file)
      [ "$#" -ge 2 ] || { echo "Missing value for $1"; exit 2; }
      if [ "$1" = --title ]; then title_arg=$2; else body_file=$2; fi
      shift 2;;
    *) echo 'Usage: bash scripts/land.sh [--title TITLE] [--body-file FILE]'; exit 2;;
  esac
done
branch=$(git rev-parse --abbrev-ref HEAD)
[ "$branch" != main ] && [ "$branch" != HEAD ] || { echo 'Land a named branch, not main or detached HEAD.'; exit 2; }
clean() {
  local status
  status=$(git status --porcelain --untracked-files=all) || return 1
  [ -z "$status" ]
}
clean || { echo 'A clean HEAD is required: commit or remove outstanding work, then rerun.'; exit 2; }
sha=$(git rev-parse HEAD)
unchanged() {
  if [ "$(git rev-parse HEAD)" = "$sha" ] && [ "$(git rev-parse --abbrev-ref HEAD)" = "$branch" ] && clean; then return; fi
  echo 'HEAD or working files changed during landing. Rerun on a clean commit.'; return 1
}
[ "$(gh api user --jq .login)" = diyanko ] || { echo "Landing requires the maintainer's GitHub account."; exit 1; }
repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner)
pr=$(gh pr list --repo "$repo" --head "$branch" --base main --state open --json number,author,isCrossRepository,isDraft \
  --jq '.[0] | if . == null then empty else [.number, .author.login, .isCrossRepository, .isDraft] | @tsv end')
number='' draft=false
if [ -n "$pr" ]; then
  read -r number author fork draft <<< "$pr"
  case "$author:$fork" in
    diyanko:false|'ibis-release[bot]:false') ;;
    *) echo "Only the maintainer's or release app's in-repository PR can land."; exit 1;;
  esac
fi
git fetch -q origin main
commit=$(git rev-list --reverse --no-merges "origin/main..$sha" | sed -n '1p')
[ -n "$commit" ] || { echo 'No non-merge commits to land.'; exit 2; }
title=${title_arg:-$(git show -s --format=%s "$commit")}
if [ -n "$number" ]; then title=$(gh pr view "$number" --repo "$repo" --json title --jq .title); fi
node scripts/pr-lint.mjs --title "$title" --branch "$branch"
# While main's latest finished push CI failed, only a fix or a revert lands. An unreadable state does not block.
red=$(gh api "repos/$repo/actions/workflows/ci.yml/runs?branch=main&event=push&status=completed&per_page=1" \
  --jq '.workflow_runs[0] | select(.conclusion == "failure" or .conclusion == "timed_out") | .html_url') || red=''
repair='^(fix|revert)[(!:]|^Revert "'
if [ -n "$red" ] && ! [[ "$title" =~ $repair ]]; then
  echo "Main is red ($red): only a fix or revert title lands until it is green."; exit 1
fi
root=$(git rev-parse --show-toplevel)
export CARGO_TARGET_DIR="$root/target"
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=1
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
# Empty values override wrappers set in Cargo's config too.
export RUSTC_WRAPPER='' RUSTC_WORKSPACE_WRAPPER=''
cargo fmt --all --check
cargo check --workspace --all-targets --locked
pnpm -C apps/desktop exec tsc --noEmit
pnpm -C apps/desktop lint
node scripts/gen.mjs --check
unchanged
git push -q -u origin "$sha:refs/heads/$branch"
body=$(mktemp)
trap 'rm -f -- "$body"' EXIT
if [ -n "$number" ]; then
  gh pr view "$number" --repo "$repo" --json body --jq .body > "$body"
elif [ -n "$body_file" ]; then cp "$body_file" "$body"
else git show -s --format=%b "$commit" > "$body"
fi
trailers=$(git log --format='%(trailers:key=Signed-off-by,key=Assisted-by,only,unfold)' "origin/main..$sha" | awk 'NF && !seen[$0]++')
trailer_args=(--if-exists addIfDifferent)
while IFS= read -r trailer; do
  [ -z "$trailer" ] || trailer_args+=(--trailer "$trailer")
done <<< "$trailers"
body_text=$(git interpret-trailers "${trailer_args[@]}" < "$body")
printf '%s\n' "$body_text" > "$body"
if [ -z "$number" ]; then
  url=$(gh pr create --repo "$repo" --base main --head "$branch" --title "$title" --body-file "$body")
  number=$url
fi
[ "$(gh pr view "$number" --repo "$repo" --json headRefOid --jq .headRefOid)" = "$sha" ] || { echo 'PR head moved; rerun.'; exit 1; }
unchanged
if [ "$draft" = true ]; then gh pr ready "$number" --repo "$repo"; fi
# Workflow, script, updater and app-permission changes merge only after another
# session records its review of this exact head on the PR. Scripts call one another,
# so every file under scripts/ counts.
guarded='^(\.github/|scripts/|apps/desktop/src-tauri/(src/updater|capabilities/|permissions/|tauri\.))'
if git diff --no-renames --name-only "origin/main...$sha" | grep -Eq "$guarded"; then
  marker="Independent review: no P1 or P2 at $sha"
  if ! gh pr view "$number" --repo "$repo" --json comments \
    --jq '.comments[] | select(.author.login == "diyanko") | .body' | grep -Fq "$marker"; then
    echo "Guarded paths changed. A session that did not write this reviews $sha and comments \"$marker\" on the PR; then rerun."
    exit 1
  fi
fi
gh pr merge "$number" --repo "$repo" --auto --squash --body-file "$body" --match-head-commit "$sha"
printf 'Auto-merge requested for %s; it merges once hosted CI result passes.\n' "$number"
