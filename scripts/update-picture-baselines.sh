#!/usr/bin/env bash
# Download a reviewed CI refresh; never render local baselines.
set -euo pipefail
[[ $# == 1 && "$1" =~ ^[0-9]+$ ]] || { echo 'Usage: bash scripts/update-picture-baselines.sh RUN_ID' >&2; exit 1; }
cd "$(dirname "$0")/.."
staging=$(mktemp -d apps/desktop/gallery/.picture-baselines.XXXXXX)
trap 'rm -rf "$staging"' EXIT
gh run download "$1" --name picture-baselines --dir "$staging"
shopt -s nullglob
pictures=("$staging"/*.png)
((${#pictures[@]})) || { echo 'Refresh artifact contains no PNGs.' >&2; exit 1; }
baseline=apps/desktop/gallery/baselines
mkdir -p "$baseline"
rm -f "$baseline"/*.png
cp "${pictures[@]}" "$baseline/"
echo 'Review the PNG diff and commit the baselines with the visual change.'
