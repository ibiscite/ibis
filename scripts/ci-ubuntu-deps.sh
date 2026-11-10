#!/usr/bin/env bash
# Install CI's Ubuntu packages for the desktop runner or web container.
set -euo pipefail
# A slow mirror still finishes; a hung one stops. Five minutes cut off slow but
# moving downloads on 2026-11-10, failing every desktop job that day.
apt=(timeout 15m apt-get -o Acquire::Retries=3 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30)

# The sources without the security pocket. CI installs only from the main
# archive, and on 2026-11-10 an unreachable security.ubuntu.com failed every
# update; an unreachable main archive still does. Each Suites line loses its
# *-security suites, and a stanza left with none is dropped.
without_security() {
  awk 'BEGIN { RS = ""; ORS = "" }
    {
      kept = ""; dropped = 0; n = split($0, lines, "\n")
      for (i = 1; i <= n; i++) {
        line = lines[i]
        if (line ~ /^Suites:/) {
          m = split(substr(line, 8), suites, /[ \t]+/); line = "Suites:"
          for (j = 1; j <= m; j++) if (suites[j] != "" && suites[j] !~ /-security$/) line = line " " suites[j]
          if (line == "Suites:") dropped = 1
        }
        kept = kept line "\n"
      }
      if (!dropped) { printf "%s%s", (printed ? "\n" : ""), kept; printed = 1 }
    }' "$1"
}

case ${1:-} in
  desktop) ;;
  web) if python3 -c 'import yaml' 2>/dev/null; then exit 0; fi ;;
  *) echo 'Usage: ci-ubuntu-deps.sh desktop|web' >&2; exit 2 ;;
esac
sources=${2:-/etc/apt/sources.list.d/ubuntu.sources}
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
without_security "$sources" > "$scratch/ubuntu.sources"
if ! grep -q '^URIs:' "$scratch/ubuntu.sources"; then
  echo 'Expected the Ubuntu archive in the sources.' >&2
  exit 1
fi
case $1 in
  desktop)
    # Keep the hosted runner's Azure-first order and its fallback mirrors.
    apt=(sudo "${apt[@]}")
    packages=(libwebkit2gtk-4.1-dev librsvg2-dev librsvg2-bin fonts-inter libsoup-3.0-dev libjavascriptcoregtk-4.1-dev patchelf xvfb xauth imagemagick fonts-noto-core fonts-noto-cjk xdotool x11-utils openbox xterm python3-pyatspi xclip at-spi2-core iproute2 lsof)
    ;;
  web)
    # Give the pinned container the runner's Azure archive as well. Retain
    # Ubuntu's suites and keyrings.
    sed -i.orig 's|^URIs: http://archive\.ubuntu\.com/ubuntu/*$|URIs: http://azure.archive.ubuntu.com/ubuntu/|' "$scratch/ubuntu.sources"
    if cmp -s "$scratch/ubuntu.sources.orig" "$scratch/ubuntu.sources"; then
      echo 'Expected Ubuntu archive missing from the web container sources.' >&2
      exit 1
    fi
    packages=(python3-yaml)
    ;;
esac
apt+=(-o "Dir::Etc::sourcelist=$scratch/ubuntu.sources" -o Dir::Etc::sourceparts=-)

"${apt[@]}" -o APT::Update::Error-Mode=any update
"${apt[@]}" install -y --no-install-recommends "${packages[@]}"
