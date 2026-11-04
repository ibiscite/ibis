#!/usr/bin/env bash
# CI's "CI result" job (ci.yml): one check that stands for every required job.
# Every unconditional job must succeed. Test and WebKit may be skipped only
# when their inputs did not change; missing classifier output fails shut.
# Stacked pull requests are refused: every pull request targets main.
#
# RESULTS: the results of changes, web, desktop and dependencies, in order.
# WEB, INTERFACE, NATIVE, CORE: the classifier's decisions.
# TEST, WEBKIT: those jobs' results. EVENT, BASE: the event and base branch.
set -e
echo "Results: $RESULTS"
if [ "$EVENT" = pull_request ] && [ "$BASE" != main ]; then
  echo "::error::This pull request targets '$BASE'. Stacked pull requests are not allowed: retarget it to main."
  exit 1
fi
set -- $RESULTS
if [ "$#" -ne 4 ]; then
  echo "::error::Expected 4 unconditional job results, got $#."
  exit 1
fi
for result in "$@"; do
  if [ "$result" != success ]; then
    echo "::error::A required job ended '$result'."
    exit 1
  fi
done
case "$EVENT:$WEB:$INTERFACE:$NATIVE:$CORE" in
  pull_request:false:false:false:false|pull_request:true:false:false:false) test=skipped webkit=skipped ;;
  pull_request:true:true:false:false|pull_request:true:true:true:false) test=skipped webkit=success ;;
  *:true:true:true:true) test=success webkit=success ;;
  *) echo "::error::Missing or invalid check selection."; exit 1 ;;
esac
if [ "$TEST" != "$test" ] || [ "$WEBKIT" != "$webkit" ]; then
  echo "::error::Expected Test '$test' and WebKit '$webkit', got '$TEST' and '$WEBKIT'."
  exit 1
fi
