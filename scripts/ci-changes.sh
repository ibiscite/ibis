#!/usr/bin/env bash
# CI's "Decide which app checks must run" step (ci.yml), from the repository
# root. Prints the check decisions and appends them to $GITHUB_OUTPUT;
# ci-paths.mjs decides, this only gathers the facts it decides from.
#
# A pull request: GitHub's file list, pinned to this event's head and base
# across pagination. A queue entry or a push to main: the difference between
# its two commits. Failed reads, changed refs or a partial list enable every check.
# A run is `proven` only when GitHub says a passing CI run checked the same tree.
#
# Release Please's own updates to its pull request run nothing: that pull
# request is checked once, when someone prepares the release (RELEASING.md).
set -eo pipefail
unset FILES DIFF PROVEN
passed() { # passed EVENT SHA: a successful CI run of EVENT for exactly SHA
  [ "$(gh api "repos/$REPO/actions/workflows/ci.yml/runs?event=$1&head_sha=$2&status=success&per_page=1" --jq '.total_count > 0')" = true ]
}
case "$EVENT" in
  pull_request)
    if [[ "$HEAD_REF" == release-please--* && "$ACTOR" == "ibis-release[bot]" ]]; then
      echo "::error::Release Please updated this pull request, so CI waits until the release is prepared. Then run: gh workflow run ci.yml --ref $HEAD_REF"
      exit 1
    fi
    if before=$(gh api "repos/$REPO/pulls/$PR" --jq '.head.sha + ":" + .base.sha') && [ "$before" = "$EXPECTED_REFS" ] &&
      gh api --paginate "repos/$REPO/pulls/$PR/files" --jq '.[] | {filename, previous_filename, status} | @json' > "$RUNNER_TEMP/changed-files.ndjson" &&
      after=$(gh api "repos/$REPO/pulls/$PR" --jq '.head.sha + ":" + .base.sha') && [ "$after" = "$EXPECTED_REFS" ]; then
      export FILES="$RUNNER_TEMP/changed-files.ndjson"
    fi
    ;;
  merge_group | push)
    if git fetch -q --no-tags --depth=1 origin "$BASE_SHA" &&
      git diff --name-status -M -z "$BASE_SHA" "$HEAD_SHA" > "$RUNNER_TEMP/changed-files.diff"; then
      export DIFF="$RUNNER_TEMP/changed-files.diff"
    fi
    # Main points at a commit the queue already checked.
    if [ "$EVENT" = push ] && passed merge_group "$HEAD_SHA"; then export PROVEN=true; fi
    # A queue entry that is exactly its pull request's head, already on top of
    # this base, holds the tree that pull request's passing run checked.
    pr=$(sed -nE 's|^(refs/heads/)?gh-readonly-queue/main/pr-([0-9]+)-[0-9a-f]{40}$|\2|p' <<<"$HEAD_REF")
    if [ "$EVENT" = merge_group ] && [ -n "$pr" ] &&
      head=$(gh api "repos/$REPO/pulls/$pr" --jq .head.sha) && [[ "$head" =~ ^[0-9a-f]{40}$ ]] &&
      [[ "$(gh api "repos/$REPO/compare/$BASE_SHA...$head" --jq .status)" =~ ^(ahead|identical)$ ]] &&
      git fetch -q --no-tags --depth=1 origin "$head" &&
      [ "$(git rev-parse "$head^{tree}")" = "$(git rev-parse "$HEAD_SHA^{tree}")" ] &&
      passed pull_request "$head"; then
      export PROVEN=true
    fi
    ;;
esac
node scripts/ci-paths.mjs | tee -a "$GITHUB_OUTPUT"
