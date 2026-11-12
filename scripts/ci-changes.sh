#!/usr/bin/env bash
# CI's "Decide which app checks must run" step (ci.yml), from the repository
# root. Prints the check decisions and appends them to $GITHUB_OUTPUT;
# ci-paths.mjs decides, this only gathers the facts it decides from.
#
# Pull requests use GitHub's file list pinned across pagination; pushes use
# their two commits. Failed reads or partial lists enable every check.
set -eo pipefail
unset FILES DIFF
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
  push)
    if git fetch -q --no-tags --depth=1 origin "$BASE_SHA" &&
      git diff --name-status -M -z "$BASE_SHA" "$HEAD_SHA" > "$RUNNER_TEMP/changed-files.diff"; then
      export DIFF="$RUNNER_TEMP/changed-files.diff"
    fi
    ;;
esac
node scripts/ci-paths.mjs | tee -a "$GITHUB_OUTPUT"
