#!/usr/bin/env bash
# Release Please's first step (release-please.yml): a push to main that makes
# a release is tagged only when the merge queue checked that exact commit. The
# release pull request's own run checks nothing (ci-changes.sh), so a release
# merged outside the queue is refused here, before any tag or draft exists.
# REPO, SHA: the repository and the pushed commit; gh reads with GH_TOKEN.
set -euo pipefail
files=$(gh api "repos/$REPO/commits/$SHA" --jq '.files[].filename')
grep -qx '.release-please-manifest.json' <<<"$files" || exit 0
checked=$(gh api "repos/$REPO/actions/workflows/ci.yml/runs?event=merge_group&head_sha=$SHA&status=success&per_page=1" --jq '.total_count > 0')
if [ "$checked" != true ]; then
  echo "::error::This release reached main without a passing merge-queue run of $SHA, so it is not tagged. Revert it, then release through the queue (RELEASING.md)."
  exit 1
fi
echo "The merge queue checked $SHA; releasing it."
