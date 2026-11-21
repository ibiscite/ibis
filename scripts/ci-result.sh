#!/usr/bin/env bash
# Required merge and publication result. Planned jobs (scripts/ci-plan.mjs) pass as
# selected-and-succeeded or not-selected-and-skipped; a selected job that was
# skipped, cancelled or failed, or an unreadable plan, never passes.
set -euo pipefail
echo "Results: $RESULTS; planned: $SELECTED; workspace: $WORKSPACE; qualification: $QUALIFICATION"
[ "$RESULTS" = 'success success success success' ] || { echo '::error::A CI job did not pass.'; exit 1; }
read -r -a planned <<< "$SELECTED"
[ "${#planned[@]}" = 4 ] || { echo '::error::Expected four planned jobs.'; exit 1; }
for entry in "${planned[@]}"; do
  case "$EVENT:${entry#*:}" in
    *:true:success|pull_request:false:skipped) ;;
    *) echo "::error::Planned job ${entry%%:*} is ${entry#*:} (selected:result)."; exit 1;;
  esac
done
case "${planned[*]%%:*}" in "rust pictures csl release") ;; *) echo '::error::Unexpected planned jobs.'; exit 1;; esac
case "$WORKSPACE:${planned[0]}" in
  false:*|true:rust:true:success) ;;
  *) echo '::error::Expected a workspace selection consistent with Rust.'; exit 1;;
esac
case "$EVENT" in
  pull_request|push) expected='skipped skipped skipped';;
  schedule|workflow_dispatch) expected='success success success';;
  *) echo '::error::Unexpected CI event.'; exit 1;;
esac
if [ "$EVENT" != pull_request ] && [ "$WORKSPACE" != true ]; then
  echo '::error::Expected the workspace outside pull requests.'
  exit 1
fi
[ "$QUALIFICATION" = "$expected" ] || { echo "::error::Expected qualification '$expected'."; exit 1; }
