#!/usr/bin/env bash
# Preview's "Decide whether to run" step of the update test (preview.yml).
# It runs from the current qualification tools in the publisher's checkout, so
# the version is that nightly's. Writes version= and, only when the previous
# nightly in previous/ can update to this build, run=true to $GITHUB_OUTPUT.
# REPO and PACKAGE come from the workflow.
set -euo pipefail
version=$(node scripts/update-feed.mjs version)
echo "version=$version" >> "$GITHUB_OUTPUT"
curl -fsSL "https://github.com/$REPO/releases/download/nightly/latest.json" -o published.json
published=$(jq -r .version published.json)
if [ ! -f previous/latest.json ] || [ ! -f "previous/$PACKAGE" ]; then
  echo "::notice::Update test skipped: the previous nightly had no update feed or no $PACKAGE."
  exit 0
fi
previous_identifier=$(cat previous/identifier.txt 2>/dev/null) || previous_identifier=unavailable
case "$previous_identifier" in
  com.ibiscite.ibis.nightly) ;;
  io.github.ibiscite.ibis.nightly) echo "::notice::Earlier app identities are outside current update qualification."; exit 0 ;;
  *) echo "::error::Previous nightly identity is unavailable or unknown; update qualification failed."; exit 1 ;;
esac
# Standalone, so an unreadable feed fails the step instead of comparing empty strings.
previous_key=$(node scripts/update-feed.mjs feed-key previous/latest.json)
published_key=$(node scripts/update-feed.mjs feed-key published.json)
if [ "$previous_key" != "$published_key" ]; then
  # Once, when the nightly key changes (ADR 0019): the previous nightly
  # rightly refuses the new key, and testers reinstall by hand.
  echo "::notice::Update test skipped: the previous nightly was signed with another key."
elif [ "$published" != "$version" ]; then
  echo "::notice::Update test skipped: the published nightly is $published, not this build ($version)."
elif [ "$(jq -r .version previous/latest.json)" = "$version" ]; then
  echo "::notice::Update test skipped: the previous nightly is this same build."
else
  echo "run=true" >> "$GITHUB_OUTPUT"
  echo "Updating $(jq -r .version previous/latest.json) to $version"
fi
