#!/usr/bin/env bash
# Check the actual PR/queue/push range, excluding only ordinary merge messages.
# The separate identity guard still validates public maintainer metadata.
set -euo pipefail
if ! [[ ${HEAD_SHA:-} =~ ^[0-9a-f]{40}$ ]]; then
  echo '::error::A full head commit ID is required for sign-offs.'; exit 1
fi
if [ -n "${BASE_SHA:-}" ] && [ "$BASE_SHA" != 0000000000000000000000000000000000000000 ]; then
  if ! [[ $BASE_SHA =~ ^[0-9a-f]{40}$ ]]; then
    echo '::error::A full base commit ID is required for sign-offs.'; exit 1
  fi
  commits=$(git rev-list "$BASE_SHA..$HEAD_SHA")
else
  commits=$(git rev-parse --verify "$HEAD_SHA^{commit}")
fi
status=0
for commit in $commits; do
  parents=$(git show -s --format=%P "$commit")
  subject=$(git show -s --format=%s "$commit")
  read -r -a parents <<< "$parents"
  if [ "${#parents[@]}" = 2 ] && [[ $subject == Merge* ]]; then continue; fi
  signoffs=$(git show -s --format='%(trailers:key=Signed-off-by,valueonly)' "$commit")
  if ! grep -Eq '^[^<>]+ <[^<>[:space:]]+@[^<>[:space:]]+>$' <<< "$signoffs"; then
    echo "::error::Commit ${commit:0:12} needs a Signed-off-by trailer."
    status=1
  fi
done
exit "$status"
