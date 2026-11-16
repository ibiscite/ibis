#!/usr/bin/env bash
# Accept the pictures a failed CI picture check rendered: never render baselines locally.
set -euo pipefail
[[ $# == 1 && "$1" =~ ^[0-9]+$ ]] || { echo 'Usage: bash scripts/update-picture-baselines.sh RUN_ID' >&2; exit 1; }
cd "$(dirname "$0")/.."
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
gh run download "$1" --name pictures --dir "$staging"
shopt -s nullglob
pictures=("$staging"/picture-actual/*.png)
((${#pictures[@]})) || { echo 'The run uploaded no pictures.' >&2; exit 1; }
rm -f apps/desktop/gallery/baselines/*.png
cp "${pictures[@]}" apps/desktop/gallery/baselines/
echo 'Review the PNG diff, then commit the baselines with the visual change.'
