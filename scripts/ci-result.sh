#!/usr/bin/env bash
# One required check; missing decisions and accidental skips fail closed.
set -euo pipefail
echo "Results: $RESULTS"
if [ "$EVENT" = pull_request ] && [ "$BASE" != main ]; then
  echo "::error::Pull requests must target main."; exit 1
fi
case "$EVENT:$WEB:$INTERFACE:$NATIVE:$CORE" in
  pull_request:false:false:false:false | push:false:false:false:false | \
  pull_request:true:false:false:false | push:true:false:false:false | \
  pull_request:true:true:false:false | push:true:true:false:false | \
  pull_request:true:true:true:false | push:true:true:true:false | \
  pull_request:true:true:true:true | push:true:true:true:true | \
  schedule:true:true:true:true | workflow_dispatch:true:true:true:true) ;;
  *) echo "::error::Missing or invalid check selection."; exit 1 ;;
esac
[ "$RESULTS" = 'success success success success' ] || { echo "::error::A required job did not pass."; exit 1; }
test=skipped
if [ "$CORE" = true ]; then test=success; fi
[ "$TEST" = "$test" ] || { echo "::error::Expected Test '$test', got '$TEST'."; exit 1; }
expected='skipped skipped skipped'
webkit=skipped
if [ "$EVENT" = schedule ] || [ "$EVENT" = workflow_dispatch ]; then
  expected='success success success'
  webkit=success
fi
[ "$WEBKIT" = "$webkit" ] && [ "$QUALIFICATION" = "$expected" ] || {
  echo "::error::Expected WebKit '$webkit' and qualification '$expected', got '$WEBKIT' and '$QUALIFICATION'."; exit 1;
}
