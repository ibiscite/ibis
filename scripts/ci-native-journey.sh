#!/usr/bin/env bash
# Preserve the harness's distinction between failure and unavailable coverage.
set -uo pipefail
status=0
"$@" || status=$?
case "$status" in
  0) result=passed ;;
  2) result=unavailable ;;
  *) result=failed ;;
esac
echo "Native journey: $result"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then printf 'Native journey: **%s**\n' "$result" >> "$GITHUB_STEP_SUMMARY"; fi
if [ "$status" = 2 ]; then echo '::warning::Native journey unavailable; no passing coverage was established.'; fi
exit "$status"
