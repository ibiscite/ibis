#!/usr/bin/env bash
# Preview's Windows packaging step, from apps/desktop. TARGET is the Rust
# target. CONFIG is the Nightly configuration and is empty for a stable build.
set -euo pipefail

options=(--ci --target "$TARGET")
if [ -n "${CONFIG:-}" ]; then options+=(--config "$CONFIG"); fi
pnpm tauri build --no-bundle "${options[@]}" -- --locked

# Tauri patches the executable for each package type and only restores it
# after a successful bundle. Save the compiler output for a fresh retry.
binary="../../target/$TARGET/release/ibis-desktop.exe"
pristine=$(mktemp)
trap 'rm -f "$pristine"' EXIT
cp "$binary" "$pristine"

bundle() {
  # tauri build adds custom-protocol before passing its options to the bundler.
  pnpm tauri bundle "${options[@]}" --features tauri/custom-protocol 2>&1 | tee build.log
}
status=0
bundle || status=$?
if [ "$status" -eq 0 ]; then exit 0; fi

asset=$(awk '$1 == "Downloading" { sub(/\r$/, "", $2); asset = $2 } END { print asset }' build.log)
case "$asset" in
  https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.13/nsis-3.13.zip|\
  https://github.com/tauri-apps/nsis-tauri-utils/releases/download/nsis_tauri_utils-v0.5.3/nsis_tauri_utils.dll) ;;
  *) exit "$status" ;;
esac
grep -Fq "failed to bundle project: \`http status: 504\`" build.log || exit "$status"

echo "::warning::Tauri's NSIS helper download returned HTTP 504. Retrying packaging once."
sleep 15
cp "$pristine" "$binary"
bundle
