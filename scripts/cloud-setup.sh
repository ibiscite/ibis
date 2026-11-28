#!/usr/bin/env bash
# One-time setup for a Linux cloud session (Ubuntu): CI's own desktop and web
# packages (scripts/ci-ubuntu-deps.sh), so `bash scripts/check.sh` runs here as
# it does on CI. Safe to rerun; installs only what is missing.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
mapfile -t packages < <(bash scripts/ci-ubuntu-deps.sh list desktop; bash scripts/ci-ubuntu-deps.sh list web)
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
