#!/usr/bin/env bash
# Summarize daily/manual qualification on main and keep one issue for failures.
set -euo pipefail
[ "${REF:-}" = refs/heads/main ] || exit 0
case ${EVENT:-} in schedule | workflow_dispatch) ;; *) exit 0 ;; esac
budget=${MACOS_BUDGET:-Unavailable (the installed macOS job did not publish a sample).}
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  printf '### Installed macOS nightly budget (report only)\n\n%s\n' "$budget" >> "$GITHUB_STEP_SUMMARY"
fi
[ "${RESULT:-}" != success ] || exit 0
title='Nightly qualification failing on main'
number=$(gh issue list --repo "$GITHUB_REPOSITORY" --state all --limit 1000 --json number,title --jq 'map(select(.title == "Nightly qualification failing on main")) | .[0].number // empty')
body=$(mktemp)
trap 'rm -f -- "$body"' EXIT
printf 'Daily/manual qualification on main did not pass. Review the failed or unavailable checks in the [qualification report](%s).\n' "$RUN_URL" > "$body"
printf '\n### Installed macOS nightly budget (report only)\n\n%s\n' "$budget" >> "$body"
if [ -n "$number" ]; then
  gh issue edit "$number" --repo "$GITHUB_REPOSITORY" --body-file "$body"
  gh issue reopen "$number" --repo "$GITHUB_REPOSITORY"
else
  gh issue create --repo "$GITHUB_REPOSITORY" --title "$title" --body-file "$body"
fi
