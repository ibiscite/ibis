#!/usr/bin/env bash
# Required merge and publication result: every job passes on all three systems;
# qualification is daily/manual. Planned jobs (scripts/ci-plan.mjs) pass as
# selected-and-succeeded or not-selected-and-skipped; a selected job that was
# skipped, cancelled or failed, or an unreadable plan, never passes.
set -euo pipefail
echo "Results: $RESULTS; planned: $SELECTED; qualification: $QUALIFICATION"
[ "$RESULTS" = 'success success success success success' ] || { echo '::error::A CI job did not pass.'; exit 1; }
[ "${CSL:-}" = success ] || { echo '::error::The CSL conformance job did not pass.'; exit 1; }
read -r -a planned <<< "$SELECTED"
[ "${#planned[@]}" = 2 ] || { echo '::error::Expected two planned jobs.'; exit 1; }
for entry in "${planned[@]}"; do
  case "${entry#*:}" in
    true:success|false:skipped) ;;
    *) echo "::error::Planned job ${entry%%:*} is ${entry#*:} (selected:result)."; exit 1;;
  esac
done
case "${planned[*]%%:*}" in "rust pictures") ;; *) echo '::error::Unexpected planned jobs.'; exit 1;; esac
case "$EVENT" in
  pull_request|push) expected='skipped skipped skipped';;
  schedule|workflow_dispatch) expected='success success success';;
  *) echo '::error::Unexpected CI event.'; exit 1;;
esac
[ "$QUALIFICATION" = "$expected" ] || { echo "::error::Expected qualification '$expected'."; exit 1; }
