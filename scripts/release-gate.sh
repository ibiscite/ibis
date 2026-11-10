#!/usr/bin/env bash
# Release Please's first step (release-please.yml). On any push to main,
# Release Please tags every merged release pull request still labelled
# "autorelease: pending". Each is released only when the merge queue checked
# its exact merged commit: the release pull request's own run checks nothing
# (ci-changes.sh), so one merged outside the queue is refused here, on this
# push and every later one, before any tag or draft exists.
# REPO: the repository; gh reads with GH_TOKEN.
set -euo pipefail
query="repo:$REPO is:pr is:merged base:main label:\"autorelease: pending\""
found=$(gh api -X GET search/issues -f q="$query" -f per_page=100 --jq '"\(.total_count) \(.incomplete_results) \(.items | length)"')
read -r total incomplete listed <<<"$found"
if [ "$incomplete" != false ] || ! [[ "$total" =~ ^[0-9]+$ ]] || [ "$total" != "$listed" ]; then
  echo "::error::The pending releases could not be listed in full, so nothing is released."
  exit 1
fi
for number in $(gh api -X GET search/issues -f q="$query" -f per_page=100 --jq '.items[].number'); do
  sha=$(gh api "repos/$REPO/pulls/$number" --jq .merge_commit_sha)
  checked=$(gh api "repos/$REPO/actions/workflows/ci.yml/runs?event=merge_group&head_sha=$sha&status=success&per_page=1" --jq '.total_count > 0')
  if ! [[ "$sha" =~ ^[0-9a-f]{40}$ ]] || [ "$checked" != true ]; then
    echo "::error::Release #$number reached main without a passing merge-queue run of its commit, so nothing is released. Revert it, then release through the queue (RELEASING.md)."
    exit 1
  fi
  echo "The merge queue checked release #$number ($sha)."
done
