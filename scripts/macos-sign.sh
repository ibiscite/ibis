#!/usr/bin/env bash
# Signs the macOS app with Ibis's own certificate (#1242), so macOS keeps the
# paste permission across updates. macOS ties that permission to the app's
# designated requirement; an ad-hoc signature makes it the build's own hash,
# while this certificate makes it "identifier … and certificate leaf = H…",
# the same for every build. The certificate is self-signed: Gatekeeper still
# asks on first launch, as it does for ad-hoc builds.
#
#   scripts/macos-sign.sh sign <dist> <platform> <identifier>
#     Re-signs Ibis-<platform>.app.tar.gz and the app in Ibis-<platform>.dmg in
#     <dist>, then rewrites its SHA256SUMS file. Needs MACOS_SIGNING_P12
#     (base64), MACOS_SIGNING_P12_PASSWORD and MACOS_SIGNING_LEAF (the
#     certificate's SHA-1), as made by scripts/macos-signing-setup.sh.
#   scripts/macos-sign.sh requirement <app> <identifier> <leaf>
#     Fails unless the app is <identifier> and its requirement pins <leaf>.
set -euo pipefail

# The certificate's common name, set by scripts/macos-signing-setup.sh.
NAME="Ibis Self-Signed"

requirement() {
  local app=$1 identifier=$2 leaf=$3 bundle actual expected
  bundle=$(plutil -extract CFBundleIdentifier raw -o - "$app/Contents/Info.plist")
  if [ "$bundle" != "$identifier" ]; then
    echo "$app is $bundle, expected $identifier." >&2
    return 1
  fi
  actual=$(codesign -d -r- "$app" 2>&1 | sed -n 's/^designated => //p')
  expected="identifier \"$identifier\" and certificate leaf = H\"$leaf\""
  if [ "$actual" != "$expected" ]; then
    echo "$app has the requirement '$actual', expected '$expected'." >&2
    return 1
  fi
}

WORK=""
KEYCHAIN=""
cleanup() {
  if [ -n "$KEYCHAIN" ]; then security delete-keychain "$KEYCHAIN" 2>/dev/null || true; fi
  if [ -n "$WORK" ]; then rm -rf "$WORK"; fi
}

sign() {
  local dist=$1 platform=$2 identifier=$3
  : "${MACOS_SIGNING_P12:?}" "${MACOS_SIGNING_P12_PASSWORD:?}" "${MACOS_SIGNING_LEAF:?}"
  local leaf password found app mount rw sectors sums
  leaf=$(printf '%s' "$MACOS_SIGNING_LEAF" | tr 'A-F' 'a-f')
  WORK=$(mktemp -d)
  KEYCHAIN="$WORK/signing.keychain-db"
  trap cleanup EXIT

  # A keychain of its own, gone when the script ends.
  password=$(uuidgen)
  security create-keychain -p "$password" "$KEYCHAIN"
  security set-keychain-settings -lut 900 "$KEYCHAIN"
  security unlock-keychain -p "$password" "$KEYCHAIN"
  printf '%s' "$MACOS_SIGNING_P12" | base64 --decode > "$WORK/identity.p12"
  security import "$WORK/identity.p12" -k "$KEYCHAIN" -P "$MACOS_SIGNING_P12_PASSWORD" -T /usr/bin/codesign > /dev/null
  rm "$WORK/identity.p12"
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$KEYCHAIN" > /dev/null
  # shellcheck disable=SC2046 # one keychain path per word
  security list-keychains -d user -s "$KEYCHAIN" $(security list-keychains -d user | tr -d '"')

  # Refuse any certificate but the pinned one before it signs anything.
  found=$(security find-certificate -c "$NAME" -Z "$KEYCHAIN" | awk '/SHA-1 hash:/ { print tolower($3) }')
  if [ "$found" != "$leaf" ]; then
    echo "The signing certificate is $found, not the pinned $leaf." >&2
    return 1
  fi

  # The update package: the app in a gzipped tar.
  mkdir "$WORK/app"
  tar -xzf "$dist/Ibis-$platform.app.tar.gz" -C "$WORK/app"
  app=$(find "$WORK/app" -maxdepth 1 -name '*.app' -print -quit)
  test -n "$app"
  # Keep the build's entitlements and hardened runtime; only the identity changes.
  codesign --force --deep --timestamp=none --preserve-metadata=entitlements,flags,runtime \
    --keychain "$KEYCHAIN" -s "$NAME" "$app"
  codesign --verify --deep --strict "$app"
  requirement "$app" "$identifier" "$leaf"
  COPYFILE_DISABLE=1 tar -czf "$dist/Ibis-$platform.app.tar.gz" -C "$WORK/app" "$(basename "$app")"

  # The disk image keeps its layout: swap the app inside a writable copy.
  rw="$WORK/rw.dmg"
  hdiutil convert "$dist/Ibis-$platform.dmg" -format UDRW -o "$rw" -quiet
  sectors=$(hdiutil resize -limits "$rw" | awk 'END { print $2 }')
  hdiutil resize -sectors $((sectors + 81920)) "$rw"
  mount="$WORK/mount"
  hdiutil attach -nobrowse -noverify -mountpoint "$mount" "$rw" > /dev/null
  rm -rf "$mount/$(basename "$app")"
  ditto "$app" "$mount/$(basename "$app")"
  requirement "$mount/$(basename "$app")" "$identifier" "$leaf"
  hdiutil detach "$mount" -quiet
  rm "$dist/Ibis-$platform.dmg"
  hdiutil convert "$rw" -format UDZO -imagekey zlib-level=9 -o "$dist/Ibis-$platform.dmg" -quiet

  # Rewrite the checksums the build wrote, outside dist so it never lists itself.
  sums=$(cd "$dist" && ls SHA256SUMS-*.txt)
  (cd "$dist" && for f in *; do [ "$f" = "$sums" ] || shasum -a 256 "$f"; done) > "$WORK/$sums"
  mv "$WORK/$sums" "$dist/$sums"
  cat "$dist/$sums"
}

case "${1:-}" in
  sign) shift; sign "$@" ;;
  requirement) shift; requirement "$@" ;;
  *) echo "Usage: $0 sign <dist> <platform> <identifier> | requirement <app> <identifier> <leaf>" >&2; exit 2 ;;
esac
