#!/usr/bin/env bash
# CI's "Decide which app checks must run" step (ci.yml), from the repository
# root. Prints core/interface/native/web decisions and appends them to
# $GITHUB_OUTPUT; ci-paths.mjs decides, this only fetches the file list.
#
# Pin the API list to this event's head and base across pagination.
# Failed reads, changed refs or a partial list enable every check.
set -eo pipefail
unset FILES
if [ "$EVENT" = pull_request ] &&
  before=$(gh api "repos/$REPO/pulls/$PR" --jq '.head.sha + ":" + .base.sha') && [ "$before" = "$EXPECTED_REFS" ] &&
  gh api --paginate "repos/$REPO/pulls/$PR/files" --jq '.[] | {filename, previous_filename, status} | @json' > "$RUNNER_TEMP/changed-files.ndjson" &&
  after=$(gh api "repos/$REPO/pulls/$PR" --jq '.head.sha + ":" + .base.sha') && [ "$after" = "$EXPECTED_REFS" ]; then
  export FILES="$RUNNER_TEMP/changed-files.ndjson"
fi
node scripts/ci-paths.mjs | tee -a "$GITHUB_OUTPUT"
