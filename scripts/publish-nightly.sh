#!/usr/bin/env bash
# Preview's "Replace the nightly pre-release" step (preview.yml), from the
# repository root: replaces the rolling nightly release TAG with dist/ and
# feed/latest.json, unless it already holds this commit SHA or a later one.
# GH_TOKEN, REPO, SHA, TAG come from the workflow.
set -euo pipefail
# An older run must not replace a newer nightly, but a newer merge
# to main must not stop this one either: with frequent merges that
# would starve the nightly. Publish unless the nightly already has
# this commit or a later one.
decision=$(node scripts/publication-guard.mjs nightly)
if [ "$decision" != publish ]; then exit 0; fi
built=$(date -u '+%Y-%m-%d %H:%M UTC')
version=$(node scripts/update-feed.mjs version)
cat > notes.md <<EOF
**Ibis Nightly $version**, built from \`${SHA:0:12}\` on main at $built. This development build is replaced after changes to main and may be unfinished or broken. For the steadier version, use the [newest numbered preview release](https://github.com/$REPO/releases/latest).

**Which file:** \`Ibis-macOS-Apple-silicon.dmg\` (M1 and later) or \`Ibis-macOS-Intel.dmg\` on a Mac, \`Ibis-Windows-setup.exe\` on Windows, \`Ibis-Linux.AppImage\` on Linux (\`.deb\` also available). It installs as **Ibis Nightly**, next to the stable Ibis and with its own settings, and can update itself if you allow update checks. The \`.app.tar.gz\` and \`latest.json\` files are for those updates.

**Command line:** \`ibis-<system>.tar.gz\` (\`ibis-Windows.zip\` on Windows) holds the \`ibis\` command for people, scripts and AI agents. Unpack it and put \`ibis\` on your PATH.

**Not signed.** macOS may say the app cannot be checked for malicious software: open System Settings → Privacy & Security and choose Open Anyway. Windows SmartScreen may warn: choose More info, then Run anyway. On Windows, if Microsoft WebView2 Runtime is missing, the installer downloads it once; Ibis works offline after setup.

Verify a download: compare \`shasum -a 256 <file>\` (macOS, Linux) or \`Get-FileHash <file>\` (Windows) with the \`SHA256SUMS-*.txt\` files below, and check provenance with \`gh attestation verify <file> --repo $REPO --source-ref refs/heads/main --signer-workflow $REPO/.github/workflows/preview.yml\`.
EOF
# The first run starts from a draft, so an empty release is never public.
if ! gh release view "$TAG" --repo "$REPO" > /dev/null 2>&1; then
  gh release create "$TAG" --repo "$REPO" --draft --prerelease --target "$SHA" --title "Ibis Nightly" --notes-file notes.md
fi
# Packages and signatures first; the update file last, so an app never
# reads a feed that names packages not yet uploaded.
gh release upload "$TAG" dist/* --repo "$REPO" --clobber
gh release upload "$TAG" feed/latest.json --repo "$REPO" --clobber
# Remove packages this build no longer produces, such as an old version number.
names=$(gh release view "$TAG" --repo "$REPO" --json assets --jq '.assets[].name')
while IFS= read -r name; do
  if [ -n "$name" ] && [ ! -e "dist/$name" ] && [ ! -e "feed/$name" ]; then gh release delete-asset "$TAG" "$name" --repo "$REPO" --yes; fi
done <<< "$names"
# Move the tag (or create it, on the first run or if it was removed),
# then publish the notes last, so they never name a commit early.
if ! gh api --method PATCH "repos/$REPO/git/refs/tags/$TAG" -f sha="$SHA" -F force=true > /dev/null; then
  gh api --method POST "repos/$REPO/git/refs" -f ref="refs/tags/$TAG" -f sha="$SHA" > /dev/null
fi
gh release edit "$TAG" --repo "$REPO" --prerelease --draft=false --latest=false --title "Ibis Nightly" --notes-file notes.md
