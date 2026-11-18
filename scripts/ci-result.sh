#!/usr/bin/env bash
# Required merge and publication result: every job passes on all three systems;
# qualification is daily/manual.
set -euo pipefail
echo "Results: $RESULTS; qualification: $QUALIFICATION"
[ "$RESULTS" = 'success success success success success success' ] || { echo '::error::A CI job did not pass.'; exit 1; }
[ "${CSL:-}" = success ] || { echo '::error::The CSL conformance job did not pass.'; exit 1; }
case "$EVENT" in
  pull_request|push) expected='skipped skipped skipped';;
  schedule|workflow_dispatch) expected='success success success';;
  *) echo '::error::Unexpected CI event.'; exit 1;;
esac
[ "$QUALIFICATION" = "$expected" ] || { echo "::error::Expected qualification '$expected'."; exit 1; }
