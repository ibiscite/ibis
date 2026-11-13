#!/usr/bin/env bash
# Keep one actionable report for failed daily/manual qualification on main.
set -euo pipefail
[ "${REF:-}" = refs/heads/main ] || exit 0
case ${EVENT:-} in schedule | workflow_dispatch) ;; *) exit 0 ;; esac
[ "${RESULT:-}" != success ] || exit 0
title='Nightly qualification failing on main'
number=$(gh issue list --repo "$GITHUB_REPOSITORY" --state all --limit 1000 --json number,title --jq 'map(select(.title == "Nightly qualification failing on main")) | .[0].number // empty')
body=$(mktemp)
trap 'rm -f -- "$body"' EXIT
printf 'Daily/manual qualification on main did not pass. Review the failed or unavailable checks in the [qualification report](%s).\n' "$RUN_URL" > "$body"
if [ -n "$number" ]; then
  gh issue edit "$number" --repo "$GITHUB_REPOSITORY" --body-file "$body"
  gh issue reopen "$number" --repo "$GITHUB_REPOSITORY"
else
  gh issue create --repo "$GITHUB_REPOSITORY" --title "$title" --body-file "$body"
fi
