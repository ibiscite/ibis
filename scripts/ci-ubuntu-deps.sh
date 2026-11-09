#!/usr/bin/env bash
# Install CI's Ubuntu packages for the desktop runner or web container.
set -euo pipefail
apt=(timeout 5m apt-get -o Acquire::Retries=1 -o Acquire::http::Timeout=15 -o Acquire::https::Timeout=15)

case ${1:-} in
  desktop)
    # Keep the hosted runner's Azure-first order and its fallback mirrors.
    apt=(sudo "${apt[@]}")
    packages=(libwebkit2gtk-4.1-dev librsvg2-dev librsvg2-bin fonts-inter libsoup-3.0-dev libjavascriptcoregtk-4.1-dev patchelf xvfb xauth imagemagick fonts-noto-core fonts-noto-cjk xdotool x11-utils openbox xterm python3-pyatspi xclip at-spi2-core iproute2 lsof)
    ;;
  web)
    if python3 -c 'import yaml' 2>/dev/null; then exit 0; fi
    # Give the pinned container the runner's Azure archive as well. Retain
    # Ubuntu's suites and keyrings, and leave the security archive in place.
    sources=${2:-/etc/apt/sources.list.d/ubuntu.sources}
    scratch=$(mktemp -d)
    trap 'rm -rf -- "$scratch"' EXIT
    sed 's|^URIs: http://archive\.ubuntu\.com/ubuntu/*$|URIs: http://azure.archive.ubuntu.com/ubuntu/|' "$sources" > "$scratch/ubuntu.sources"
    if cmp -s "$sources" "$scratch/ubuntu.sources"; then
      echo 'Expected Ubuntu archive missing from the web container sources.' >&2
      exit 1
    fi
    apt+=(-o "Dir::Etc::sourcelist=$scratch/ubuntu.sources" -o Dir::Etc::sourceparts=-)
    packages=(python3-yaml)
    ;;
  *) echo 'Usage: ci-ubuntu-deps.sh desktop|web' >&2; exit 2 ;;
esac

"${apt[@]}" -o APT::Update::Error-Mode=any update
"${apt[@]}" install -y --no-install-recommends "${packages[@]}"
