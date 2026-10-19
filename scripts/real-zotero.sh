#!/usr/bin/env bash
# Runs the real-Zotero test (crates/ibis-zotero-local/tests/real_zotero.rs)
# against an actual Zotero: downloads the current Linux release, starts it
# with a fresh, throwaway profile and data directory and the local API
# turned on, then runs the test. Linux only; needs xvfb-run and network
# access to zotero.org. Zotero listens on a port of its own (not 23119), so a
# Zotero you have open is never touched; nothing outside a temporary
# directory is written.
#
#   scripts/real-zotero.sh
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
# A port of its own, so a Zotero the developer has open is never written to.
port=23129
if curl -s -m 2 -o /dev/null "http://127.0.0.1:$port/"; then
  echo "something already listens on port $port; stop it first" >&2; exit 1
fi
work=$(mktemp -d)
zotero_pid=
cleanup() {
  # Zotero runs in its own process group, so this also stops its helpers.
  [ -n "$zotero_pid" ] && kill -- "-$zotero_pid" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

echo "Downloading Zotero for Linux"
curl -fsSL --retry 3 -o "$work/zotero.tar" \
  "https://www.zotero.org/download/client/dl?channel=release&platform=linux-x86_64"
tar -xf "$work/zotero.tar" -C "$work"
app=$(find "$work" -maxdepth 2 -type f -name zotero -perm -u+x | head -n 1)
[ -n "$app" ] || { echo "the Zotero download has no zotero program" >&2; exit 1; }
grep -m1 '^Version=' "$(dirname "$app")/app/application.ini" 2>/dev/null \
  || grep -m1 '^Version=' "$(dirname "$app")/application.ini" 2>/dev/null || true

mkdir -p "$work/profile" "$work/data"
cat > "$work/profile/user.js" <<EOF
user_pref("extensions.zotero.dataDir", "$work/data");
user_pref("extensions.zotero.useDataDir", true);
user_pref("extensions.zotero.httpServer.localAPI.enabled", true);
user_pref("extensions.zotero.httpServer.port", $port);
user_pref("extensions.zotero.lastViewedFolder", "L1");
user_pref("extensions.zotero.firstRun2", false);
user_pref("extensions.zotero.firstRunGuidance", false);
user_pref("extensions.zotero.sync.autoSync", false);
user_pref("extensions.zotero.automaticScraperUpdates", false);
user_pref("extensions.zotero.reportTranslationFailure", false);
user_pref("app.update.enabled", false);
user_pref("app.update.auto", false);
EOF

echo "Starting Zotero"
setsid xvfb-run -a "$app" -profile "$work/profile" > "$work/zotero.log" 2>&1 &
zotero_pid=$!

for _ in $(seq 120); do
  if curl -fsS -m 5 -o /dev/null "http://127.0.0.1:$port/connector/ping" 2>/dev/null; then break; fi
  if ! kill -0 "$zotero_pid" 2>/dev/null; then
    echo "Zotero exited during startup:" >&2; tail -n 50 "$work/zotero.log" >&2; exit 1
  fi
  sleep 1
done
curl -fsS -m 5 -o /dev/null "http://127.0.0.1:$port/connector/ping" \
  || { echo "Zotero did not start within two minutes:" >&2; tail -n 50 "$work/zotero.log" >&2; exit 1; }
# The connector answers before the main window has opened; give the
# library view a moment so new items land in My Library.
sleep 10

cd "$root"
if ! IBIS_ZOTERO_PORT=$port IBIS_THROWAWAY_ZOTERO=1 cargo test -p ibis-zotero-local --locked --test real_zotero -- --ignored --nocapture; then
  echo "Zotero log:" >&2; tail -n 80 "$work/zotero.log" >&2; exit 1
fi
