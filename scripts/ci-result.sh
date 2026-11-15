#!/usr/bin/env bash
# Required merge and publication result. Pull requests may run Ubuntu only;
# main and daily/manual runs need all three systems; qualification is daily/manual.
set -euo pipefail
echo "Results: $RESULTS; platforms: ${PLATFORMS:-}; qualification: $QUALIFICATION"
[ "$RESULTS" = 'success success success success success success' ] || { echo '::error::A CI job did not pass.'; exit 1; }
all='["ubuntu-24.04","macos-15","windows-2025"]'
case "$EVENT" in
  pull_request) expected='skipped skipped skipped skipped' platforms="$all|[\"ubuntu-24.04\"]";;
  push) expected='skipped skipped skipped skipped' platforms=$all;;
  schedule|workflow_dispatch) expected='success success success success' platforms=$all;;
  *) echo '::error::Unexpected CI event.'; exit 1;;
esac
[[ "|$platforms|" == *"|${PLATFORMS:-}|"* ]] || { echo "::error::Unexpected platforms for $EVENT."; exit 1; }
[ "$QUALIFICATION" = "$expected" ] || { echo "::error::Expected qualification '$expected'."; exit 1; }
