#!/usr/bin/env bash
# Preview's "Capture the picker (Linux and macOS)" step (preview.yml), from the
# repository root: start the release app and watch it stay offline for 60
# seconds, then take a screenshot. Only the offline observer is privileged.
# TARGET: the Rust target; CONFIG: the Ibis Nightly config, empty for Ibis.
set -eo pipefail
mkdir -p screenshots
capture() {
  local binary=ibis-desktop
  if [ -n "$CONFIG" ]; then
    binary=$(node -e 'console.log(JSON.parse(require("node:fs").readFileSync(process.argv[1])).mainBinaryName || "ibis-desktop")' "$CONFIG")
  fi
  # Complete Linux fd/socket attribution needs a privileged observer.
  local observer=()
  if [ "$RUNNER_OS" = Linux ]; then observer=(sudo -n); fi
  python3 scripts/check-offline.py --record-baseline screenshots/offline-$RUNNER_OS.json
  "target/$TARGET/release/$binary" > screenshots/startup-$RUNNER_OS.log 2>&1 &
  app_pid=$!
  trap 'kill "$app_pid" 2>/dev/null || true' EXIT
  # Observe startup as well as the delayed background services.
  "${observer[@]}" python3 scripts/check-offline.py --pid "$app_pid" --baseline screenshots/offline-$RUNNER_OS.json --seconds 60
  if ! kill -0 "$app_pid" 2>/dev/null; then
    echo "::error::The picker exited during startup; see the startup log."
    exit 1
  fi
  if [ "$RUNNER_OS" = Linux ]; then
    import -window root screenshots/picker-Linux.png || echo "::warning::Linux screenshot capture failed."
  else
    screencapture -x screenshots/picker-macOS.png || echo "::warning::macOS screenshot capture failed."
  fi
}
if [ "$RUNNER_OS" = Linux ]; then
  xvfb-run -a -s '-screen 0 1280x800x24' dbus-run-session -- bash -e -c "$(declare -f capture); capture"
else
  capture
fi
