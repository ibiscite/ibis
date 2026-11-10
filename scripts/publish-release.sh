#!/usr/bin/env bash
# Preview's "Publish the verified release" step (preview.yml), from the
# repository root: publishes the drafted numbered release TAG with the
# verified packages in dist/. Every refusal and failed read comes before the
# first change on GitHub. GH_TOKEN, TAG, REPO, ACTOR come from the workflow.
set -euo pipefail
# An older tag must not take Latest back. A failed lookup stops here,
# before any draft is published or its packages are replaced.
latest=$(gh api "repos/$REPO/releases/latest" --jq .tag_name)
latest_flag=--latest=false
if [ "$TAG" = "$latest" ] || node scripts/update-feed.mjs newer "${TAG#v}" "${latest#v}"; then
  latest_flag=--latest
fi
# Release Please drafts the release with its generated change list;
# scripts/release-notes.mjs puts the download table and install notes
# in front, drops the commit hashes (#450) and names the citation styles
# whose output changed, from the style-changes job (#19).
if gh release view "$TAG" --repo "$REPO" --json body,isDraft > release.json; then
  if [ "$(jq -r .isDraft release.json)" != true ]; then
    echo "::error::The release is already public; refusing to replace its packages."
    exit 1
  fi
  jq -r .body release.json > generated-notes.md
  node scripts/release-notes.mjs "$TAG" generated-notes.md style-changes/notes.md > release-notes.md
  gh release upload "$TAG" dist/* --repo "$REPO" --clobber
  gh release edit "$TAG" --repo "$REPO" --prerelease=false --draft=false "$latest_flag" --title "Ibis ${TAG#v}" --notes-file release-notes.md
elif [[ "$ACTOR" == *"[bot]" ]]; then
  # Release Please pushes the tag before it drafts the release. A bot
  # tag with no draft means that step failed; publishing would drop
  # the change list, so stop and let a maintainer rerun Release Please.
  echo "::error::No draft release for $TAG from $ACTOR; rerun Release Please, then this workflow."
  exit 1
else
  : > generated-notes.md
  node scripts/release-notes.mjs "$TAG" generated-notes.md style-changes/notes.md > release-notes.md
  gh release create "$TAG" dist/* --repo "$REPO" --prerelease=false --draft --title "Ibis ${TAG#v}" --notes-file release-notes.md --verify-tag
  gh release edit "$TAG" --repo "$REPO" --prerelease=false --draft=false "$latest_flag"
fi
