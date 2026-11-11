#!/usr/bin/env bash
# Split time-dependent advisory results from the dependency policy's fixed rules.
set -euo pipefail
case ${1:-} in
  policy)
    blocking=true
    case ${EVENT:-} in
      pull_request | merge_group)
        # PRs compare with their merge base; queue entries compare with current
        # main. No rename detection: removing a manifest counts as a change too.
        range="${BASE_SHA:-}..${HEAD_SHA:-}"
        if [ "$EVENT" = pull_request ]; then range="${BASE_SHA:-}...${HEAD_SHA:-}"; fi
        if [[ ${BASE_SHA:-} =~ ^[0-9a-f]{40}$ && ${HEAD_SHA:-} =~ ^[0-9a-f]{40}$ ]] &&
          files=$(git diff --name-only --no-renames "$range" -- '**/Cargo.toml' Cargo.toml '**/Cargo.lock' Cargo.lock deny.toml); then
          if [ -z "$files" ]; then blocking=false; fi
        else
          echo '::warning::Dependency changes could not be read; advisories remain blocking.' >&2
        fi
        ;;
      *)
        # Releases are prepared manually on the Release Please branch. Tags
        # and the weekly check enforce the full advisory policy.
        if [[ ${REF:-} != refs/tags/* && ${HEAD_REF:-} != release-please--* && ${REF:-} != refs/heads/release-please--* && ${EVENT:-} != release ]] &&
          ! { [ "${EVENT:-}" = schedule ] && [ "${SCHEDULE:-}" = '43 5 * * 1' ]; }; then
          blocking=false
        fi
        ;;
    esac
    echo "blocking=$blocking"
    ;;
  report)
    case ${OUTCOME:-} in
      success) exit 0 ;;
      failure) ;;
      *) echo '::error::Missing advisory check result.'; exit 1 ;;
    esac
    echo '::warning::RustSec advisory check failed; see the advisory step for the report.'
    # Only the weekly main run writes issues. One exact title, updated in place,
    # also covers a database outage without calling it a confirmed vulnerability.
    if [ "${EVENT:-}" = schedule ] && [ "${SCHEDULE:-}" = '43 5 * * 1' ] && [ "${REF:-}" = refs/heads/main ]; then
      title='RustSec advisory check failing on main'
      number=$(gh issue list --repo "$GITHUB_REPOSITORY" --state open --limit 1000 --json number,title --jq 'map(select(.title == "RustSec advisory check failing on main")) | .[0].number // empty')
      body=$(mktemp)
      trap 'rm -f -- "$body"' EXIT
      printf 'The weekly full advisory check on main failed. Review the RustSec findings or database error in the [advisory check report](%s).\n' "$RUN_URL" > "$body"
      if [ -n "$number" ]; then
        gh issue edit "$number" --repo "$GITHUB_REPOSITORY" --body-file "$body"
      else
        gh issue create --repo "$GITHUB_REPOSITORY" --title "$title" --body-file "$body"
      fi
    fi
    if [ "${BLOCKING:-true}" != false ]; then exit 1; fi
    ;;
  *) echo 'Usage: ci-advisories.sh policy|report' >&2; exit 2 ;;
esac
