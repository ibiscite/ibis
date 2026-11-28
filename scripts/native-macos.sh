#!/usr/bin/env bash
# Repo-only native macOS build. The Swift package links the workspace's Rust archive.
set -euo pipefail
if [[ "$(uname -s)" != Darwin ]]; then
  printf '%s\n' 'The Ibis native app requires macOS.' >&2
  exit 1
fi

mode=${1:-}
case "$mode" in
  build|test|run|dev|shots) shift ;;
  *)
    printf '%s\n' 'Usage: scripts/native-macos.sh build|test|run [--file LIBRARY] [--show]' \
      '       scripts/native-macos.sh dev     rebuild, quit the running copy, open the picker on the review library' \
      '       scripts/native-macos.sh shots [DIR]  screenshot every review state in light and dark' >&2
    exit 2
    ;;
esac

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
package="$root/native/macos"
app="$root/target/native-macos/Ibis.app"
export CARGO_TARGET_DIR="$root/target"
# The app needs the macOS 26 SDK (Swift 6.2) even though it runs on macOS 15.
# Hosted runners and older Macs may default to an older Xcode beside a newer one.
if [[ -z "${DEVELOPER_DIR:-}" ]] && ! swift --version 2>/dev/null | grep -Eq 'Swift version (6\.[2-9]|[7-9])'; then
  newest=$(ls -d /Applications/Xcode_26*.app /Applications/Xcode-26*.app 2>/dev/null | sort -V | tail -1 || true)
  if [[ -n "$newest" ]]; then
    export DEVELOPER_DIR="$newest/Contents/Developer"
  else
    printf '%s\n' 'The native app needs Xcode 26 or its Command Line Tools (Swift 6.2).' >&2
    exit 1
  fi
fi
# SwiftPM records the deployment target as the SDK version, which makes macOS
# draw the pre-26 look. Record the SDK the app is really built with.
sdk_version=$(xcrun --sdk macosx --show-sdk-version)
link_sdk=(-Xlinker -platform_version -Xlinker macos -Xlinker 15.0 -Xlinker "$sdk_version")
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-3}"

cargo build --locked -p ibis -p ibis-native-ffi
# SwiftPM treats the Rust archive path as a linker flag. Invalidate its
# cached link when the archive changes without a Swift source change.
fingerprint=$(shasum -a 256 "$CARGO_TARGET_DIR/debug/libibis_native_ffi.a")
link_stamp="$package/.build/ibis-rust-archive.sha256"
if [[ ! -f "$link_stamp" || "$(cat "$link_stamp")" != "$fingerprint" ]]; then
  swift package --package-path "$package" clean
  mkdir -p "$package/.build"
  printf '%s\n' "$fingerprint" > "$link_stamp"
fi
if [[ "$mode" == test ]]; then
  cargo test --locked -p ibis-native-ffi
  swift test --package-path "$package" --jobs 3 "${link_sdk[@]}"
  exit
fi

swift build --package-path "$package" --jobs 3 "${link_sdk[@]}"
swift_bin=$(swift build --package-path "$package" --show-bin-path "${link_sdk[@]}")
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$package/Info.plist" "$app/Contents/Info.plist"
cp "$swift_bin/IbisNative" "$app/Contents/MacOS/IbisNative"
# The daemon handshake uses the CLI binary's size and modification time.
cp -p "$CARGO_TARGET_DIR/debug/ibis" "$app/Contents/MacOS/ibis"
cp "$root/apps/desktop/src-tauri/icons/icon.icns" "$app/Contents/Resources/"
cp "$root/brand/exports/png/ibis-menu-bar.png" "$app/Contents/Resources/"
codesign --force --sign - "$app"
codesign --verify --strict "$app"
printf 'Built %s\n' "$app"

review="$package/Review.csljson"

if [[ "$mode" == dev ]]; then
  pkill -x IbisNative 2>/dev/null && sleep 0.5 || true
  open -a "$app" --args --file "$review" --show
  exit
fi

if [[ "$mode" == shots ]]; then
  # One isolated profile and daemon for every state; nothing touches your own settings.
  out=${1:-"$root/target/native-macos/shots"}
  mkdir -p "$out"
  config=$(mktemp -d)
  states=(results search draft details peek no-matches larger choose
          settings-general settings-appearance settings-library)
  # The picker takes keys without activating, so wait while someone is typing.
  idle() { ioreg -c IOHIDSystem | awk '/HIDIdleTime/ {print int($NF / 1000000000); exit}'; }
  for appearance in light dark; do
    for state in "${states[@]}"; do
      for _ in $(seq 60); do (( $(idle) >= 3 )) && break; sleep 1; done
      args=(--state "$state" --appearance "$appearance")
      [[ "$state" == choose ]] || args+=(--file "$review")
      log="$config/$state.log"
      IBIS_CONFIG_DIR="$config" "$app/Contents/MacOS/IbisNative" "${args[@]}" >"$log" 2>/dev/null &
      pid=$!
      for _ in $(seq 100); do grep -q '^ready' "$log" && break; sleep 0.2; done
      # `ready WINDOW` names one window; `ready rect X,Y,W,H` a screen area
      # holding the picker with its Peek or popover.
      ready=$(awk '/^ready/ {print $2, $3; exit}' "$log")
      if [[ "$ready" == rect* ]]; then
        screencapture -o -x -R "${ready#rect }" "$out/$appearance-$state.png"
      elif [[ -n "$ready" ]]; then
        screencapture -o -x -l "${ready% }" "$out/$appearance-$state.png"
      else
        printf 'No window for %s (%s)\n' "$state" "$appearance" >&2
      fi
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    done
  done
  printf 'Screenshots in %s\n' "$out"
  exit
fi

if [[ "$mode" == run ]]; then
  launch=(-a "$app")
  if [[ -n "${IBIS_CONFIG_DIR:-}" ]]; then
    launch+=(--env "IBIS_CONFIG_DIR=$IBIS_CONFIG_DIR")
  fi
  args=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --file)
        if [[ $# -lt 2 ]]; then printf '%s\n' 'Provide a path after --file.' >&2; exit 2; fi
        case "$2" in
          /*) args+=(--file "$2") ;;
          *) args+=(--file "$root/$2") ;;
        esac
        shift 2
        ;;
      --show) args+=(--show); shift ;;
      *) printf 'Unknown argument: %s\n' "$1" >&2; exit 2 ;;
    esac
  done
  if [[ ${#args[@]} -eq 0 ]]; then
    open "${launch[@]}"
  else
    open "${launch[@]}" --args "${args[@]}"
  fi
fi
