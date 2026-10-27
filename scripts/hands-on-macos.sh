#!/bin/bash
# The maintainer's hands-on check on a Mac, as one command:
#   curl -fsSL https://raw.githubusercontent.com/ibiscite/ibis/main/scripts/hands-on-macos.sh | bash
# It installs the newest Ibis Nightly and the ibis command line, sets up
# synthetic test files, does the file moves itself and asks only yes/no
# questions. Answers go to ~/Desktop/ibis-test/results.txt and the clipboard.
# It never reads your Zotero library or documents; notes you type are yours.
set -euo pipefail

repo=https://github.com/ibiscite/ibis
raw=https://raw.githubusercontent.com/ibiscite/ibis/main
dir="$HOME/Desktop/ibis-test"
app="/Applications/Ibis Nightly.app"
[ "$(uname -s)" = Darwin ] || { echo "This check is for macOS."; exit 1; }
case "$(uname -m)" in
  arm64) system=macOS-Apple-silicon target=aarch64-apple-darwin ;;
  *) system=macOS-Intel target=x86_64-apple-darwin ;;
esac
exec 3</dev/tty
results=()
say() { printf '\n\033[1m%s\033[0m\n' "$*"; }
# ask "what to do" "question": records yes, no (with a one-line note) or skipped.
ask() {
  say "$1"
  local answer note=""
  while true; do
    printf '%s [y = yes, n = no, s = skip] ' "$2"
    read -r answer <&3
    case "$answer" in
      y | Y) answer=yes; break ;;
      n | N) answer=no; printf 'What did you see? (one line, no private names or paths) '; read -r note <&3; break ;;
      s | S) answer=skipped; break ;;
    esac
  done
  results+=("$answer: $2${note:+ ($note)}")
}
summon() { echo "Press Command+Shift+Y (or click the Ibis menu bar icon and choose to open the picker)."; }

say "1/4 Getting the newest Ibis Nightly and the test files"
mkdir -p "$dir/bin"
cd "$dir"
for file in "Ibis-$system.dmg" "ibis-$system.tar.gz" "SHA256SUMS-$target.txt"; do
  curl -fsSL -o "$file" "$repo/releases/download/nightly/$file"
done
grep -E " (Ibis-$system\.dmg|ibis-$system\.tar\.gz)$" "SHA256SUMS-$target.txt" | shasum -a 256 -c -
for f in real-world damaged empty; do curl -fsSL -o "$f.bib" "$raw/fixtures/bibtex/$f.bib"; done
tar -xzf "ibis-$system.tar.gz" -C bin ibis
xattr -d com.apple.quarantine bin/ibis 2>/dev/null || true

say "2/4 Installing Ibis Nightly"
osascript -e 'quit app "Ibis Nightly"' 2>/dev/null || true
sleep 2
mount=$(mktemp -d)
hdiutil attach -quiet -nobrowse -mountpoint "$mount" "Ibis-$system.dmg"
rm -rf "$app"
ditto "$mount/Ibis Nightly.app" "$app"
hdiutil detach -quiet "$mount"
xattr -dr com.apple.quarantine "$app" 2>/dev/null || true
version=$(defaults read "$app/Contents/Info" CFBundleShortVersionString)
echo "Installed Ibis Nightly $version."

say "3/4 The command line"
bin/ibis --version
if bin/ibis search garcia --file real-world.bib >/dev/null && bin/ibis cite --help >/dev/null; then
  results+=("yes: the ibis command line searches a .bib file")
else
  results+=("no: the ibis command line could not search real-world.bib")
fi
echo "Example: $dir/bin/ibis search garcia --file $dir/real-world.bib"

say "4/4 Questions (about 10 minutes). Answer y, n or s."
open "$app"
sleep 3
summon
ask "Choose Choose a library and pick real-world.bib in $dir. Search: garcia sleep, then press Enter with TextEdit open in front of Ibis." \
  "Was a correct citation pasted into TextEdit (or copied, with a note saying why not)?"
ask "Search garcia sleep again, press Shift+Enter on two references, then Enter." \
  "Did you get one citation with both references?"
ask "Search: memory p 12, then press Enter." \
  "Did Ibis show it read page 12, and does the citation have the page?"
ask "In the Copy as menu, choose APA, then search: sleep and press Enter." \
  "Does the APA citation look right?"
ask "Put Safari or TextEdit in full screen, summon Ibis, copy, then press Escape." \
  "Did Ibis appear over the full-screen app, and were you back in it afterwards?"

printf '\n@article{handson2026,\n  author = {Example, Ada},\n  title = {A test entry added while Ibis runs},\n  journal = {Journal of Tests},\n  year = {2026}\n}\n' >> real-world.bib
ask "I just added a reference to real-world.bib. Summon Ibis and search: handson2026." \
  "Was it found without reloading?"
mv real-world.bib moved.bib
ask "I just renamed real-world.bib. Summon Ibis." \
  "Does Ibis say clearly what happened and how to fix it?"
mv moved.bib real-world.bib
ask "I renamed it back. Use Try again in Ibis, then search garcia." \
  "Does search work again?"
ask "Choose Choose a library and pick damaged.bib, then empty.bib, then real-world.bib again." \
  "Did damaged.bib open the readable references, and empty.bib give clear next steps?"

osascript -e 'quit app "Ibis Nightly"' 2>/dev/null || true
sleep 2
settings="$HOME/Library/Application Support/com.ibiscite.ibis.nightly"
[ -f "$settings/settings.json" ] || settings="$HOME/Library/Application Support/io.github.ibiscite.ibis.nightly"
if [ -f "$settings/settings.json" ]; then
  cp "$settings/settings.json" "$dir/settings-before-check.json"
  echo broken > "$settings/settings.json"
  open "$app"
  sleep 3
  summon
  ask "I damaged Ibis's settings file on purpose and reopened Ibis." \
    "Does Ibis still have real-world.bib chosen, and say it restored your settings?"
else
  results+=("skipped: no settings file found to damage")
fi

ask "If you use Zotero: open Zotero, summon Ibis, choose Use Zotero, then search and copy from your own library." \
  "Do the results look right?"
ask "Now quit Zotero completely, quit Ibis from its menu bar icon, open Ibis again and search your library." \
  "Can you still search and copy your Zotero references with Zotero closed?"
ask "Turn on VoiceOver (Command+F5), summon Ibis, type a search, press the down arrow, then turn VoiceOver off." \
  "Did you hear the search field, the number of results and the selected reference?"
ask "In the picker's More menu, turn on Larger text, look around, then turn it off." \
  "Is everything readable and reachable with nothing cut off?"
ask "Use Ibis normally for a minute." \
  "Does it feel fast and easy to read?"

{
  echo "Ibis hands-on check, $(date -u +%Y-%m-%dT%H:%MZ)"
  echo "macOS $(sw_vers -productVersion), $(uname -m), Ibis Nightly $version"
  printf '%s\n' "${results[@]}"
} > results.txt
pbcopy < results.txt
say "Done. Thank you!"
cat results.txt
echo
echo "The results are on your clipboard and in $dir/results.txt. Paste them in the chat."
