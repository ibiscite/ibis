#!/usr/bin/env bash
# Preview's "Keep the previous nightly for the update test" step
# (preview.yml), from the repository root: downloads into previous/ the files
# of the nightly this run replaces. Never fails for missing evidence; nothing
# downloaded runs. GH_TOKEN and REPO come from the workflow.
set -euo pipefail
mkdir previous
echo "Files of the nightly this run replaces, if it had an update feed." > previous/README.txt
# The update packages the update test installs: one per system it runs on.
previous_sha=$(gh api "repos/$REPO/git/ref/tags/nightly" --jq .object.sha) || previous_sha=""
gh release download nightly --repo "$REPO" --pattern latest.json --pattern Ibis-Linux.AppImage \
  --pattern Ibis-Windows-setup.exe --pattern Ibis-macOS-Apple-silicon.app.tar.gz --dir previous ||
  echo "No previous nightly with an update feed; the update test will be skipped."
if [ -f previous/latest.json ]; then
  # Historical evidence is optional; it must never block publishing
  # or imply that an unknown/changed source qualified an update.
  echo unavailable > previous/identifier.txt
  if [[ "$previous_sha" =~ ^[a-f0-9]{40}$ ]] &&
    gh api "repos/$REPO/contents/scripts/update-feed.mjs?ref=$previous_sha" --jq .content | base64 --decode > "$RUNNER_TEMP/previous-update-feed.mjs" &&
    previous_identifier=$(node scripts/update-feed.mjs previous-identity "$RUNNER_TEMP/previous-update-feed.mjs") &&
    current_sha=$(gh api "repos/$REPO/git/ref/tags/nightly" --jq .object.sha) &&
    [ "$previous_sha" = "$current_sha" ]; then
    echo "$previous_identifier" > previous/identifier.txt
  else
    echo "::notice::Previous nightly identity could not be established; update qualification will be skipped."
  fi
fi
