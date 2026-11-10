#!/usr/bin/env bash
# Release Please's first step (release-please.yml). On any push to main,
# Release Please tags every merged release pull request still labelled
# "autorelease: pending". Each is released only when the merge queue checked
# its exact merged commit: the release pull request's own run checks nothing
# (ci-changes.sh), so one merged outside the queue is refused here, on this
# push and every later one, before any tag or draft exists.
# REPO: owner/name; gh reads with GH_TOKEN. Run from the repository root.
set -euo pipefail
# A label set in the config would change what Release Please tags; this gate
# knows only its default, so it refuses rather than check the wrong pull requests.
if [ "$(jq -r '.label // empty' release-please-config.json)" != "" ]; then
  echo "::error::release-please-config.json sets its own label; update scripts/release-gate.sh to match it."
  exit 1
fi
# Read directly, not through search, whose index can lag behind a merge.
query='query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) {
  pullRequests(states: MERGED, baseRefName: "main", labels: ["autorelease: pending"], first: 100) {
    totalCount nodes { number mergeCommit { oid } } } } }'
pending=$(gh api graphql -f query="$query" -F owner="${REPO%/*}" -F name="${REPO#*/}" \
  --jq '.data.repository.pullRequests | "\(.totalCount) \(.nodes | length)", (.nodes[] | "\(.number) \(.mergeCommit.oid)")')
read -r total listed <<<"$(head -n 1 <<<"$pending")"
if ! [[ "$total" =~ ^[0-9]+$ ]] || [ "$total" != "$listed" ]; then
  echo "::error::The pending releases could not be listed in full, so nothing is released."
  exit 1
fi
while read -r number sha; do
  [ -n "$number" ] || continue
  checked=false
  if [[ "$sha" =~ ^[0-9a-f]{40}$ ]]; then
    checked=$(gh api "repos/$REPO/actions/workflows/ci.yml/runs?event=merge_group&head_sha=$sha&status=success&per_page=1" --jq '.total_count > 0')
  fi
  if [ "$checked" != true ]; then
    echo "::error::Release #$number reached main without a passing merge-queue run of its commit, so nothing is released. Revert it, then release through the queue (RELEASING.md)."
    exit 1
  fi
  echo "The merge queue checked release #$number ($sha)."
done <<<"$(tail -n +2 <<<"$pending")"
