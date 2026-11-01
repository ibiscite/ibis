#!/bin/sh
# Compiles brand/exports/Ibis.icon to the Assets.car the app ships, so macOS 26
# and later draw the Default, Dark, Clear and Tinted appearances. Run it on a Mac
# with Xcode 26 after brand/build.py changes the icon; older systems use icon.icns.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
xcrun actool "$root/brand/exports/Ibis.icon" --compile "$out" --platform macosx \
  --minimum-deployment-target 11.0 --app-icon Ibis \
  --output-partial-info-plist "$out/partial.plist" >/dev/null
mkdir -p "$root/apps/desktop/src-tauri/icons/macos"
cp "$out/Assets.car" "$root/apps/desktop/src-tauri/icons/macos/Assets.car"
