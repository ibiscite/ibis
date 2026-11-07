#!/usr/bin/env bash
# Preview's "Collect packages and checksums" step (preview.yml), from the
# repository root: copy the built packages into dist/ under plain names that
# say which computer each is for, add the command-line archive and write
# SHA256SUMS-$TARGET.txt. Names carry no version, so a link to the nightly file
# never changes.
# TARGET: the app's Rust target; CLI_TARGET: the command line's; RUNNER_OS.
set -eo pipefail
bundle="target/$TARGET/release/bundle"
mkdir -p dist
find "$bundle" -maxdepth 2 -type f \( -name '*.dmg' -o -name '*.msi' -o -name '*-setup.exe' -o -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' \) -exec cp {} dist/ \;
if [ -z "$(ls dist)" ]; then
  echo "no packages were produced in $bundle" >&2
  exit 1
fi
# The macOS update package: the app in a gzipped tar, as the updater expects.
if [ -d "$bundle/macos" ]; then
  app=$(find "$bundle/macos" -maxdepth 1 -name '*.app' -print -quit)
  COPYFILE_DISABLE=1 tar -czf "dist/$(basename "$app").tar.gz" -C "$bundle/macos" "$(basename "$app")"
fi
case "$TARGET" in
  aarch64-apple-darwin) platform=macOS-Apple-silicon ;;
  x86_64-apple-darwin) platform=macOS-Intel ;;
  x86_64-pc-windows-msvc) platform=Windows ;;
  x86_64-unknown-linux-gnu) platform=Linux ;;
  *) echo "no download name for $TARGET" >&2; exit 1 ;;
esac
for f in dist/*; do
  case "$f" in
    *-setup.exe) name="Ibis-$platform-setup.exe" ;;
    *.app.tar.gz) name="Ibis-$platform.app.tar.gz" ;;
    *) name="Ibis-$platform.${f##*.}" ;;
  esac
  if [ -e "dist/$name" ]; then echo "two packages would both be called $name" >&2; exit 1; fi
  mv "$f" "dist/$name"
done
# The command line with its licence, in the archive each system unpacks
# without extra tools.
mkdir cli
cp LICENSE NOTICE.md cli/
if [ "$RUNNER_OS" = Windows ]; then
  cp "target/$CLI_TARGET/release/ibis.exe" cli/
  (cd cli && 7z a -tzip "../dist/ibis-$platform.zip" ibis.exe LICENSE NOTICE.md > /dev/null)
else
  cp "target/$CLI_TARGET/release/ibis" cli/
  COPYFILE_DISABLE=1 tar -czf "dist/ibis-$platform.tar.gz" -C cli ibis LICENSE NOTICE.md
fi
# Write the list outside dist first, so it never lists itself.
sums="SHA256SUMS-$TARGET.txt"
(cd dist && for f in *; do
  if command -v sha256sum >/dev/null; then sha256sum "$f"; else shasum -a 256 "$f"; fi
done) > "$sums"
mv "$sums" "dist/$sums"
cat "dist/$sums"
