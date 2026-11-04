#!/usr/bin/env bash
# Preview's "Move the stable update feed to this release" step (preview.yml),
# from the repository root: the release behind the fixed FEED tag mirrors
# dist/ and feed/latest.json, unless its feed is already as new. Every failed
# read comes before the first change. GH_TOKEN, REPO, SHA, TAG, FEED come
# from the workflow.
set -euo pipefail
(cd dist && sha256sum --check SHA256SUMS-*.txt)
decision=$(node scripts/publication-guard.mjs stable feed/latest.json)
if [ "$decision" != publish ]; then exit 0; fi
cat > feed-notes.md <<EOF
**Not a release.** This entry holds the files behind Ibis's permanent download links and \`latest.json\`, the update feed the app reads. It currently mirrors [Ibis ${TAG#v}](https://github.com/$REPO/releases/tag/$TAG). For release notes and how to verify a download, see [the latest release](https://github.com/$REPO/releases/latest).
EOF
if ! gh release view "$FEED" --repo "$REPO" > /dev/null 2>&1; then
  gh release create "$FEED" --repo "$REPO" --draft --prerelease --target "$SHA" --title "Ibis update feed" --notes-file feed-notes.md
fi
# Packages first, the feed last; then remove files from older releases.
gh release upload "$FEED" dist/* --repo "$REPO" --clobber
gh release upload "$FEED" feed/latest.json --repo "$REPO" --clobber
names=$(gh release view "$FEED" --repo "$REPO" --json assets --jq '.assets[].name')
while IFS= read -r name; do
  if [ -n "$name" ] && [ ! -e "dist/$name" ] && [ ! -e "feed/$name" ]; then gh release delete-asset "$FEED" "$name" --repo "$REPO" --yes; fi
done <<< "$names"
gh release edit "$FEED" --repo "$REPO" --prerelease --draft=false --latest=false --title "Ibis update feed" --notes-file feed-notes.md
