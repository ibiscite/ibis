#!/usr/bin/env bash
# Preview's "Sign the update packages" steps (preview.yml), from the repository
# root: sign the four update packages in dist/, whose checksums and provenance
# the step before just verified, and write feed/latest.json listing them.
#   scripts/sign-update-packages.sh release   TAG names the release (v1.2.3)
#   scripts/sign-update-packages.sh nightly   SHA names the commit
# TAURI_SIGNING_PRIVATE_KEY(_PASSWORD) and REPO come from the workflow. The
# feed refuses a signature made with any key other than the app's own.
set -euo pipefail
case "${1:-}" in
  release)
    # The feed's version must be the one the app reports, or it would offer
    # the same release again and again.
    workspace=$(node scripts/update-feed.mjs release)
    if [ "${TAG#v}" != "$workspace" ]; then
      echo "::error::$TAG does not match the workspace version $workspace."
      exit 1
    fi
    version="${TAG#v}"
    url="https://github.com/$REPO/releases/download/$TAG"
    notes="Ibis $version. Release notes: https://github.com/$REPO/releases/tag/$TAG" ;;
  nightly)
    version=$(node scripts/update-feed.mjs version)
    url="https://github.com/$REPO/releases/download/nightly"
    notes="Nightly build of ${SHA:0:12} on main." ;;
  *) echo "usage: $0 release|nightly" >&2; exit 2 ;;
esac
for file in Ibis-macOS-Apple-silicon.app.tar.gz Ibis-macOS-Intel.app.tar.gz Ibis-Windows-setup.exe Ibis-Linux.AppImage; do
  apps/desktop/node_modules/.bin/tauri signer sign --app-version "$version" "dist/$file" > /dev/null
done
mkdir feed
mv dist/*.sig feed/
node scripts/update-feed.mjs manifest feed "$url" "$version" "$notes"
rm feed/*.sig
cat feed/latest.json
