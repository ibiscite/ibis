#!/usr/bin/env bash
# Preview's "Build the command line" step (preview.yml), from the repository
# root. The command line ships on its own too, for people, scripts and AI
# agents. It runs here where it can, so a binary that cannot start is never
# published (Intel macOS is cross-built on Apple silicon).
# TARGET: the app's Rust target; RUNNER_OS; GITHUB_ENV receives CLI_TARGET.
set -eo pipefail
APP_TARGET=$TARGET
if [ "$RUNNER_OS" = Windows ]; then
  # Link the C runtime in, as the app does, so ibis.exe runs on a PC without
  # the Visual C++ Redistributable.
  export RUSTFLAGS="-C target-feature=+crt-static"
fi
# Linux builds for musl: a static binary runs on every distribution, NixOS
# included, with no system libraries.
if [ "$RUNNER_OS" = Linux ]; then TARGET=x86_64-unknown-linux-musl; fi
echo "CLI_TARGET=$TARGET" >> "$GITHUB_ENV"
cargo build --release --locked -p ibis --target "$TARGET"
if [ "$RUNNER_OS" = Linux ] && ! file "target/$TARGET/release/ibis" | grep -q 'static'; then
  echo "::error::The Linux command line is not static."
  exit 1
fi
if [ "$RUNNER_OS" = Windows ] && grep -qai 'vcruntime140.dll' "target/$TARGET/release/ibis.exe"; then
  echo "::error::ibis.exe still needs vcruntime140.dll."
  exit 1
fi
if [ "$TARGET" != x86_64-apple-darwin ]; then
  "target/$TARGET/release/ibis" --version
fi

# Tauri strips the app target suffix when placing a sidecar next to the app.
# Linux bundles the same static bytes as a resource for per-user Settings setup.
mkdir -p apps/desktop/src-tauri/binaries
suffix=
if [ "$RUNNER_OS" = Windows ]; then suffix=.exe; fi
cp "target/$TARGET/release/ibis$suffix" "apps/desktop/src-tauri/binaries/ibis-$APP_TARGET$suffix"
if [ "$RUNNER_OS" = Linux ]; then
  cp "target/$TARGET/release/ibis" apps/desktop/src-tauri/binaries/ibis
fi
