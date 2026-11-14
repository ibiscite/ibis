#!/usr/bin/env bash
# Required merge and publication result; qualification is daily/manual.
set -euo pipefail
echo "Results: $RESULTS; qualification: $QUALIFICATION"
[ "$RESULTS" = 'success success success success success' ] || { echo '::error::A confirmation job did not pass.'; exit 1; }
case "$EVENT" in
  push|pull_request|merge_group) expected='skipped skipped skipped skipped';;
  schedule|workflow_dispatch) expected='success success success success';;
  *) echo '::error::Unexpected CI event.'; exit 1;;
esac
[ "$QUALIFICATION" = "$expected" ] || { echo "::error::Expected qualification '$expected'."; exit 1; }
