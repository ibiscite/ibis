#!/usr/bin/env bash
# CI's "CI result" job (ci.yml): one check that stands for every required job.
# Every unconditional job must succeed. A job may be skipped only when the
# classifier said its inputs did not change, or, for a queue entry or a push
# to main, that a passing run already checked this exact tree (PROVEN). Missing
# classifier output fails shut. Stacked pull requests are refused: every pull
# request targets main.
#
# RESULTS: the results of changes, web, desktop and dependencies, in order.
# WEB, INTERFACE, NATIVE, CORE, PROVEN: the classifier's decisions.
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
  [[ "$result" =~ ^(success|skipped)$ ]] || { echo "::error::A required job ended '$result'."; exit 1; }
done
if [ "$PROVEN" = true ]; then
  case "$EVENT:$WEB:$INTERFACE:$NATIVE:$CORE" in
    merge_group:false:false:false:false | push:false:false:false:false) ;;
    *) echo "::error::Only a queue entry or a push to main can reuse an earlier run, and then it runs nothing else."; exit 1 ;;
  esac
  expected="success skipped skipped success" test=skipped webkit=skipped
else
  [ "$PROVEN" = false ] || { echo "::error::Missing or invalid check selection."; exit 1; }
  expected="success success success success"
  # Pull requests and queue entries run what their files can affect; anything else runs everything.
  case "$EVENT:$WEB:$INTERFACE:$NATIVE:$CORE" in
    pull_request:false:false:false:false | merge_group:false:false:false:false | \
    pull_request:true:false:false:false | merge_group:true:false:false:false) test=skipped webkit=skipped ;;
    pull_request:true:true:false:false | merge_group:true:true:false:false | \
    pull_request:true:true:true:false | merge_group:true:true:true:false) test=skipped webkit=success ;;
    *:true:true:true:true) test=success webkit=success ;;
    *) echo "::error::Missing or invalid check selection."; exit 1 ;;
  esac
fi
if [ "$*" != "$expected" ]; then
  echo "::error::Expected the changes, web, desktop and dependency jobs to end '$expected', got '$*'."
  exit 1
fi
if [ "$TEST" != "$test" ] || [ "$WEBKIT" != "$webkit" ]; then
  echo "::error::Expected Test '$test' and WebKit '$webkit', got '$TEST' and '$WEBKIT'."
  exit 1
fi
