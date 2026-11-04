#!/usr/bin/env bash
# One-time setup for a Linux cloud session (Ubuntu): the libraries ibis-desktop
# needs to build and test, so desktop Rust is checked before pushing, not only
# in CI. Safe to rerun; installs only what is missing.
set -euo pipefail
packages=(libwebkit2gtk-4.1-dev librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libayatana-appindicator3-dev python3-yaml)
missing=()
for package in "${packages[@]}"; do
  dpkg-query -W -f='${Status}' "$package" 2>/dev/null | grep -q "install ok installed" || missing+=("$package")
done
if ((${#missing[@]})); then
  sudo=""; [ "$(id -u)" = 0 ] || sudo="sudo"
  $sudo apt-get update -qq
  DEBIAN_FRONTEND=noninteractive $sudo apt-get install -y -qq --no-install-recommends "${missing[@]}"
fi
echo "Cloud setup ready. Build with: export CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=<worktree>/target"
