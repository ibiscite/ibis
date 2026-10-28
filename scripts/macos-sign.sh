#!/usr/bin/env bash
# Sign the macOS update archive and DMG with the pinned self-signed identity.
# Usage: sign <dist> <platform> <identifier> | requirement <app> <identifier> <leaf>
set -euo pipefail
set +x
umask 077

run_step() {
  local step=$1 status
  shift
  "$@" || {
    status=$?
    # Labels are fixed strings: never log arguments containing passwords.
    echo "$step failed (exit $status)." >&2
    return "$status"
  }
}

validate() {
  case "$1" in com.ibiscite.ibis|com.ibiscite.ibis.nightly) ;; *) echo 'Unknown Ibis identifier.' >&2; return 1 ;; esac
  [[ "$2" =~ ^[0-9a-f]{40}$ ]] || { echo 'Expected a lowercase SHA-1 certificate pin.' >&2; return 1; }
}

requirement() {
  local app=$1 identifier=$2 leaf=$3 bundle actual expected status
  validate "$identifier" "$leaf"
  # Displaying a requirement alone does not verify the signature or its pin.
  run_step 'codesign verify' codesign --verify --deep --strict -R="identifier \"$identifier\" and certificate leaf = H\"$leaf\"" "$app"
  bundle=$(plutil -extract CFBundleIdentifier raw -o - "$app/Contents/Info.plist")
  if [ "$bundle" != "$identifier" ]; then
    echo "$app is $bundle, expected $identifier." >&2
    return 1
  fi
  actual=$(run_step 'codesign display' codesign -d -r- "$app" 2>&1) || {
    status=$?
    printf '%s\n' "$actual" >&2
    return "$status"
  }
  actual=$(printf '%s\n' "$actual" | sed -n 's/^designated => //p')
  expected="identifier \"$identifier\" and certificate leaf = H\"$leaf\""
  if [ "$actual" != "$expected" ]; then
    echo "$app has the requirement '$actual', expected '$expected'." >&2
    return 1
  fi
}

WORK=""
KEYCHAIN=""
MOUNT=""
ORIGINAL_KEYCHAINS=""
KEYCHAIN_LIST_SAVED=0
set_keychain_search_list() {
  local step=$1 path
  shift
  # security prints one quoted path per line. Preserve spaces and literal
  # shell characters without eval; positional arguments also handle an empty list.
  while IFS= read -r path; do
    [ -n "$path" ] || continue
    path=${path#*\"}
    set -- "$@" "${path%\"*}"
  done <<< "$ORIGINAL_KEYCHAINS"
  run_step "$step" security list-keychains -d user -s "$@"
}

cleanup() {
  local status=$?
  trap - EXIT
  if [ -n "$MOUNT" ]; then hdiutil detach "$MOUNT" -quiet || status=1; fi
  if [ -n "$KEYCHAIN" ]; then run_step 'security delete-keychain' security delete-keychain "$KEYCHAIN" || status=1; fi
  if [ "$KEYCHAIN_LIST_SAVED" = 1 ]; then set_keychain_search_list 'security restore search list' || status=1; fi
  if [ -n "$WORK" ]; then rm -rf "$WORK"; fi
  exit "$status"
}

sign() {
  local dist=$1 platform=$2 identifier=$3
  : "${MACOS_SIGNING_P12:?}" "${MACOS_SIGNING_P12_PASSWORD:?}" "${MACOS_SIGNING_LEAF:?}"
  local leaf password found app rw sectors sums target
  leaf=$(printf '%s' "$MACOS_SIGNING_LEAF" | tr 'A-F' 'a-f')
  validate "$identifier" "$leaf"
  case "$platform" in
    macOS-Apple-silicon) target=aarch64-apple-darwin ;;
    macOS-Intel) target=x86_64-apple-darwin ;;
    *) echo 'Unknown macOS platform.' >&2; return 1 ;;
  esac
  WORK=$(mktemp -d)
  trap cleanup EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM

  # Extract before importing any identity. Python's data filter rejects paths
  # and links escaping the destination, devices and other special members.
  mkdir "$WORK/app"
  python3 - "$dist/Ibis-$platform.app.tar.gz" "$WORK/app" <<'PY'
import pathlib, sys, tarfile
with tarfile.open(sys.argv[1], 'r:gz') as archive:
    archive.extractall(sys.argv[2], filter='data')
apps = list(pathlib.Path(sys.argv[2]).iterdir())
if len(apps) != 1 or not apps[0].is_dir() or apps[0].is_symlink() or apps[0].suffix != '.app':
    raise SystemExit('Expected exactly one app bundle')
PY
  app=$(find "$WORK/app" -maxdepth 1 -name '*.app' -print)
  test "$(plutil -extract CFBundleIdentifier raw -o - "$app/Contents/Info.plist")" = "$identifier"

  # No dependency installation or app execution. Save before creation, which
  # can itself add a search-list entry. Cleanup restores this list on every exit.
  ORIGINAL_KEYCHAINS=$(run_step 'security save search list' security list-keychains -d user)
  KEYCHAIN_LIST_SAVED=1
  KEYCHAIN="$WORK/signing.keychain-db"
  password=$(uuidgen)
  run_step 'security create-keychain' security create-keychain -p "$password" "$KEYCHAIN"
  # codesign also consults the user search list even with --keychain.
  set_keychain_search_list 'security add search list' "$KEYCHAIN"
  run_step 'security set-keychain-settings' security set-keychain-settings -lut 900 "$KEYCHAIN"
  run_step 'security unlock-keychain' security unlock-keychain -p "$password" "$KEYCHAIN"
  printf '%s' "$MACOS_SIGNING_P12" | base64 --decode > "$WORK/identity.p12"
  run_step 'security import' security import "$WORK/identity.p12" -k "$KEYCHAIN" -P "$MACOS_SIGNING_P12_PASSWORD" -T /usr/bin/codesign > /dev/null
  rm "$WORK/identity.p12"
  unset MACOS_SIGNING_P12 MACOS_SIGNING_P12_PASSWORD
  run_step 'security set-key-partition-list' security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$KEYCHAIN" > /dev/null
  found=$(run_step 'security find-certificate' security find-certificate -a -Z "$KEYCHAIN" | awk '/SHA-1 hash:/ { print tolower($3) }')
  if [ "$found" != "$leaf" ]; then
    echo 'The signing keychain does not hold exactly the pinned certificate.' >&2
    return 1
  fi
  # Public facts only: shows whether the key is paired with the pinned certificate.
  security find-identity -p codesigning "$KEYCHAIN" >&2 || true
  run_step 'codesign sign' codesign --force --deep --timestamp=none --preserve-metadata=entitlements,flags,runtime \
    --keychain "$KEYCHAIN" -s "$leaf" "$app"
  requirement "$app" "$identifier" "$leaf"
  run_step 'security delete-keychain' security delete-keychain "$KEYCHAIN"
  KEYCHAIN=""
  COPYFILE_DISABLE=1 tar -czf "$WORK/update.tar.gz" -C "$WORK/app" "$(basename "$app")"

  # Keep the existing disk image layout. Build both outputs before replacing
  # either input; a failure cannot upload a half-signed package set.
  rw="$WORK/rw.dmg"
  hdiutil convert "$dist/Ibis-$platform.dmg" -format UDRW -o "$rw" -quiet
  sectors=$(hdiutil resize -limits "$rw" | awk 'END { print $2 }')
  [[ "$sectors" =~ ^[0-9]+$ ]] || { echo 'Invalid disk image size.' >&2; return 1; }
  hdiutil resize -sectors "$((sectors + 81920))" "$rw"
  MOUNT="$WORK/mount"
  hdiutil attach -nobrowse -noverify -mountpoint "$MOUNT" "$rw" > /dev/null
  rm -rf "${MOUNT:?}/$(basename "$app")"
  ditto "$app" "$MOUNT/$(basename "$app")"
  requirement "$MOUNT/$(basename "$app")" "$identifier" "$leaf"
  hdiutil detach "$MOUNT" -quiet
  MOUNT=""
  hdiutil convert "$rw" -format UDZO -imagekey zlib-level=9 -o "$WORK/signed.dmg" -quiet
  mv "$WORK/update.tar.gz" "$dist/Ibis-$platform.app.tar.gz"
  mv "$WORK/signed.dmg" "$dist/Ibis-$platform.dmg"
  sums="SHA256SUMS-$target.txt"
  (cd "$dist" && for f in *; do [ "$f" = "$sums" ] || shasum -a 256 "$f"; done) > "$WORK/$sums"
  mv "$WORK/$sums" "$dist/$sums"
  cat "$dist/$sums"
}

case "${1:-}" in
  sign) [ "$#" -eq 4 ] || exit 2; shift; sign "$@" ;;
  requirement) [ "$#" -eq 4 ] || exit 2; shift; requirement "$@" ;;
  *) echo "Usage: $0 sign <dist> <platform> <identifier> | requirement <app> <identifier> <leaf>" >&2; exit 2 ;;
esac
