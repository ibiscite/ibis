#!/bin/bash
# The maintainer's hands-on check on a Mac, as one command:
#   curl -fsSL https://raw.githubusercontent.com/ibiscite/ibis/main/scripts/hands-on-macos.sh | bash
# Installs the newest Ibis Nightly and CLI. Synthetic checks run with temporary
# settings; the original settings and backup are saved before launch and restored
# on exit. An explicit opt-in Zotero check shows your own library in the picker.
# Only answers and typed notes go to a fresh ~/Desktop/ibis-test-*/results.txt
# and the clipboard. Keep notes free of library content, private names and paths.
set -euo pipefail
set +x
umask 077

repo=https://github.com/ibiscite/ibis
raw=https://raw.githubusercontent.com/ibiscite/ibis/main
app="/Applications/Ibis Nightly.app"
settings="$HOME/Library/Application Support/com.ibiscite.ibis.nightly"
dir="" mount="" stage="" isolated=0 swapping=0 had_app=0
results=()
fail() { echo "$*" >&2; return 1; }
say() { printf '\n\033[1m%s\033[0m\n' "$*"; }
# Check ancestors too: refusing only the leaf still permits redirected writes.
no_links() {
  local path=$1
  case "$path" in /*) ;; *) path="$PWD/$path" ;; esac
  while [ "$path" != / ]; do
    [ ! -L "$path" ] || { fail "Refusing a symlinked destination."; return 1; }
    path=${path%/*}
    [ -n "$path" ] || path=/
  done
}
regular_destination() {
  no_links "$1" || return 1
  [ ! -e "$1" ] || [ -f "$1" ] || { fail "Expected a regular file destination."; return 1; }
}
absent_destination() {
  no_links "$1" || return 1
  [ ! -e "$1" ] || { fail "Refusing an occupied destination."; return 1; }
}
choose_assets() {
  local chip
  chip=$(uname -m)
  if [ "$(sysctl -in sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then chip=arm64; fi
  case "$chip" in
    arm64) system=macOS-Apple-silicon target=aarch64-apple-darwin ;;
    x86_64) system=macOS-Intel target=x86_64-apple-darwin ;;
    *) fail "Unsupported Mac architecture." ;;
  esac
}
verify_downloads() {
  regular_destination "$dir/checked-sums.txt" || return 1
  # shasum alone accepts an omitted file and warns without failing on bad lines.
  awk -v dmg="Ibis-$system.dmg" -v cli="ibis-$system.tar.gz" '
    { name = $NF; sub(/^\*/, "", name) }
    name == dmg || name == cli {
      count[name]++
      if (NF != 2 || length($1) != 64 || $1 ~ /[^0-9a-fA-F]/ ||
          $0 !~ /^[0-9a-fA-F]+ [ *][^[:space:]]+$/) bad = 1
      else print
    }
    END { if (bad || count[dmg] != 1 || count[cli] != 1) exit 1 }
  ' "$dir/SHA256SUMS-$target.txt" > "$dir/checked-sums.txt" || { fail "Missing, duplicate or malformed asset checksum."; return 1; }
  (cd "$dir" && shasum -a 256 --strict -c checked-sums.txt)
}
stop_app() {
  local attempt status
  osascript -e 'if application "Ibis Nightly" is running then tell application "Ibis Nightly" to quit' 2>/dev/null || true
  for ((attempt=0; attempt<10; attempt++)); do
    status=0
    pgrep -f '/Ibis Nightly[.]app/Contents/MacOS/' >/dev/null || status=$?
    case "$status" in 1) return 0 ;; 0) sleep 1 ;; *) break ;; esac
  done
  fail "Could not verify Ibis Nightly has stopped; leaving saved settings untouched."
}
restore_settings() {
  local name
  stop_app || return 1
  for name in settings.json settings.json.bak; do
    regular_destination "$settings/$name" || return 1
    regular_destination "$dir/original-settings/$name" || return 1
    if [ -f "$dir/original-settings/$name" ]; then
      cp -p "$dir/original-settings/$name" "$settings/$name" || return 1
    else
      rm -f "$settings/$name" || return 1
    fi
  done
  isolated=0
}
cleanup() {
  local status=$?
  trap - EXIT
  trap '' INT TERM
  set +e
  if [ "$isolated" = 1 ]; then
    restore_settings || { status=1; echo "Restore the saved files in $dir/original-settings after quitting Ibis Nightly." >&2; }
  fi
  if [ -n "$mount" ]; then
    hdiutil detach -quiet "$mount" || hdiutil detach -quiet -force "$mount" || status=1
  fi
  if [ -n "$stage" ]; then
    if [ "$swapping" = 1 ]; then
      if [ -d "$stage/previous.app" ]; then
        if no_links "$app" && no_links "$stage/previous.app" && rm -rf "$app" && mv "$stage/previous.app" "$app"; then
          swapping=0
        else
          status=1
          echo "Installation rollback failed; the previous app is in $stage/previous.app." >&2
        fi
      elif [ "$had_app" = 0 ]; then
        no_links "$app" && rm -rf "$app" && swapping=0
      else
        swapping=0 # Moving the original failed; it is still in place.
      fi
    fi
    if [ "$swapping" = 0 ]; then no_links "$stage" && rm -rf "$stage" || status=1; fi
  fi
  exit "$status"
}
launch() {
  local name
  stop_app
  isolated=1 # Set before the first write, including a partially failed copy.
  for name in settings.json settings.json.bak; do
    regular_destination "$settings/$name"
    cp "$dir/synthetic-settings.json" "$settings/$name"
  done
  if [ "${1:-}" = recovery ]; then printf 'broken\n' > "$settings/settings.json"; fi
  open "$app"
  sleep 3
}
# ask "what to do" "question": records yes, no (with a one-line note) or skipped.
ask() {
  say "$1"
  local note=""
  while true; do
    printf '%s [y = yes, n = no, s = skip] ' "$2"
    read -r answer <&3 || exit 1
    case "$answer" in
      y | Y) answer=yes; break ;;
      n | N) answer=no; printf 'What did you see? (one line, no library content, private names or paths) '; read -r note <&3 || exit 1; break ;;
      s | S) answer=skipped; break ;;
    esac
  done
  results+=("$answer: $2${note:+ ($note)}")
}
opt_in_zotero() {
  say "Optional Zotero check: Ibis will read your own Zotero library and show it in the picker. Only yes/no answers and your typed notes are recorded; keep notes free of library content."
  local reply
  while true; do
    printf 'Allow the Zotero open/closed check? [y/n] '
    read -r reply <&3 || exit 1
    case "$reply" in
      y | Y) results+=("yes: allowed the personal Zotero check"); return 0 ;;
      n | N) results+=("no: declined the personal Zotero check"); return 1 ;;
    esac
  done
}
summon() { echo "Press Command+Shift+Y (or click the Ibis menu bar icon and choose to open the picker)."; }

# Main: never source or run the installer in automated tests.
[ "$(uname -s)" = Darwin ] || { fail "This check is for macOS."; exit 1; }
choose_assets
exec 3</dev/tty
no_links "$HOME/Desktop"
dir=$(mktemp -d "$HOME/Desktop/ibis-test-$(date +%Y%m%d-%H%M%S).XXXXXX")
no_links "$dir"
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
stop_app
no_links "$settings"
mkdir -p "$settings"
no_links "$dir/original-settings"
no_links "$dir/bin"
mkdir "$dir/original-settings" "$dir/bin"
for name in settings.json settings.json.bak; do
  regular_destination "$settings/$name"
  regular_destination "$dir/original-settings/$name"
  if [ -f "$settings/$name" ]; then cp -p "$settings/$name" "$dir/original-settings/$name"; fi
done

say "1/4 Getting the newest Ibis Nightly and the synthetic test files"
cd "$dir"
for file in "Ibis-$system.dmg" "ibis-$system.tar.gz" "SHA256SUMS-$target.txt"; do
  regular_destination "$dir/$file"
  curl -fsSL -o "$file" "$repo/releases/download/nightly/$file"
done
verify_downloads
for file in real-world damaged empty; do
  regular_destination "$dir/$file.bib"
  curl -fsSL -o "$file.bib" "$raw/fixtures/bibtex/$file.bib"
done
regular_destination "$dir/synthetic-settings.json"
osascript -l JavaScript -e 'function run(a) { return JSON.stringify({schema_version:2, library:{bibtex:a[0]}, update_checks:false}); }' "$dir/real-world.bib" > synthetic-settings.json
regular_destination "$dir/bin/ibis"
# Stream only the expected member into a regular file; never extract archive links.
tar -xOzf "ibis-$system.tar.gz" ibis > bin/ibis
chmod 700 bin/ibis

say "2/4 Installing Ibis Nightly"
stop_app
no_links "$app"
if [ -e "$app" ]; then
  [ -d "$app" ] || { fail "Expected an app bundle directory."; exit 1; }
  had_app=1
fi
no_links /Applications
stage=$(mktemp -d /Applications/.ibis-hands-on.XXXXXX)
no_links "$stage"
mount="$dir/mount"
no_links "$mount"
mkdir "$mount"
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$mount" "Ibis-$system.dmg"
no_links "$mount/Ibis Nightly.app"
no_links "$stage/new.app"
ditto "$mount/Ibis Nightly.app" "$stage/new.app"
codesign --verify --deep --strict "$stage/new.app"
[ "$(defaults read "$stage/new.app/Contents/Info" CFBundleIdentifier)" = com.ibiscite.ibis.nightly ]
version=$(defaults read "$stage/new.app/Contents/Info" CFBundleShortVersionString)
xattr -dr com.apple.quarantine "$stage/new.app" 2>/dev/null || true
hdiutil detach -quiet "$mount"
mount=""
# Both renames stay on /Applications' filesystem; EXIT rolls back either failure.
stop_app
no_links "$app"
absent_destination "$stage/previous.app"
if [ "$had_app" = 1 ]; then
  swapping=1
  mv "$app" "$stage/previous.app"
fi
absent_destination "$app"
swapping=1
mv "$stage/new.app" "$app"
codesign --verify --deep --strict "$app"
swapping=0
echo "Installed Ibis Nightly $version."

say "3/4 The command line"
bin/ibis --version
if bin/ibis search garcia --file real-world.bib >/dev/null && bin/ibis cite --help >/dev/null; then
  results+=("yes: the ibis command line searches a .bib file")
else
  results+=("no: the ibis command line could not search the synthetic fixture")
fi
echo "Example: $dir/bin/ibis search garcia --file $dir/real-world.bib"

say "4/4 Questions (about 10 minutes). Answer y, n or s."
launch
summon
ask "The synthetic real-world.bib is already chosen. Search: garcia sleep, then press Enter with TextEdit open in front of Ibis." \
  "Was a correct citation pasted into TextEdit (or copied, with a note saying why not)?"
ask "Search sleep, press Shift+Enter on two references, then Enter." \
  "Did you get one citation with both references?"
ask "Search: memory p 12, then press Enter." \
  "Did Ibis show it read page 12, and does the citation have the page?"
ask "In the Copy as menu, choose APA, then search: sleep and press Enter." \
  "Does the APA citation look right?"
ask "Put Safari or TextEdit in full screen, summon Ibis, copy, then press Escape." \
  "Did Ibis appear over the full-screen app, and were you back in it afterwards?"

regular_destination "$dir/real-world.bib"
printf '\n@article{handson2026,\n  author = {Example, Ada},\n  title = {A test entry added while Ibis runs},\n  journal = {Journal of Tests},\n  year = {2026}\n}\n' >> real-world.bib
ask "I just added a reference to real-world.bib. Summon Ibis and search: handson2026." \
  "Was it found without reloading?"
regular_destination "$dir/moved.bib"
mv real-world.bib moved.bib
ask "I just renamed real-world.bib. Summon Ibis." \
  "Does Ibis say clearly what happened and how to fix it?"
regular_destination "$dir/real-world.bib"
mv moved.bib real-world.bib
ask "I renamed it back. Use Try again in Ibis, then search garcia." \
  "Does search work again?"
ask "Choose Choose a library and pick damaged.bib, then empty.bib, then real-world.bib again in the test folder." \
  "Did damaged.bib open the readable references, and empty.bib give clear next steps?"

launch recovery
summon
ask "I seeded both settings files with real-world.bib, damaged the primary, and reopened Ibis." \
  "Does Ibis have the synthetic real-world.bib chosen, and say it restored your settings?"
ask "Turn on VoiceOver (Command+F5), summon Ibis, type a search, press the down arrow, then turn VoiceOver off." \
  "Did you hear the search field, the number of results and the selected reference?"
ask "In the picker's More menu, turn on Larger text, look around, then turn it off." \
  "Is everything readable and reachable with nothing cut off?"
ask "Use Ibis with the synthetic library for a minute." \
  "Does it feel fast and easy to read?"

if opt_in_zotero; then
  ask "Open Zotero, summon Ibis, choose Use Zotero, then search and copy from your own library. Keep library content out of your notes." \
    "Do the Zotero results look right with Zotero open?"
  ask "Quit Zotero completely before answering; leave it closed for the next check." \
    "Is Zotero closed and ready for the offline check?"
  if [ "$answer" = yes ]; then
    launch
    summon
    ask "Ibis restarted with synthetic settings. Choose Use Zotero again, then search and copy while Zotero stays closed." \
      "Can you search and copy your Zotero references with Zotero closed?"
  fi
fi
restore_settings
regular_destination "$dir/results.txt"
{
  echo "Ibis hands-on check, $(date -u +%Y-%m-%dT%H:%MZ)"
  echo "macOS $(sw_vers -productVersion), $system, Ibis Nightly $version"
  printf '%s\n' "${results[@]}"
} > results.txt
pbcopy < results.txt
say "Done. Thank you! Original Nightly settings restored; Ibis is stopped."
cat results.txt
echo
echo "The results are on your clipboard and in $dir/results.txt. Paste only results.txt in the chat; original-settings contains private configuration."
