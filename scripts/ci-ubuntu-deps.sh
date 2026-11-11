#!/usr/bin/env bash
# Install CI's Ubuntu packages for the desktop runner or web container.
set -euo pipefail
# A slow mirror still finishes; a hung one stops. Five minutes cut off slow but
# moving downloads on 2026-11-10, failing every desktop job that day.
apt=(timeout 15m apt-get -o Acquire::Retries=3 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30)

profile=${1:-}
if [ "$profile" = cache-key ]; then profile=${2:-}; fi
case $profile in
  desktop)
    packages=(libwebkit2gtk-4.1-dev librsvg2-dev librsvg2-bin fonts-inter libsoup-3.0-dev libjavascriptcoregtk-4.1-dev patchelf xvfb xauth imagemagick fonts-noto-core fonts-noto-cjk xdotool x11-utils openbox xterm python3-pyatspi xclip at-spi2-core iproute2 lsof)
    ;;
  web) packages=(python3-yaml) ;;
  *) echo 'Usage: ci-ubuntu-deps.sh [cache-key] desktop|web' >&2; exit 2 ;;
esac
# The cache is stable across runs and changes exactly with this profile's list.
if [ "$1" = cache-key ]; then
  printf '%s\n' "${packages[@]}" | sha256sum | awk '{ print "key=" $1 }'
  exit 0
fi
if [ "$profile" = web ] && python3 -c 'import yaml' 2>/dev/null; then exit 0; fi
sources=${2:-/etc/apt/sources.list.d/ubuntu.sources}
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
# Container images otherwise delete downloaded .debs in post-invoke hooks,
# even when Keep-Downloaded-Packages is true. Override only for these commands.
cat > "$scratch/keep-downloads.conf" <<'EOF'
#clear DPkg::Post-Invoke;
#clear APT::Update::Post-Invoke;
APT::Keep-Downloaded-Packages "true";
Binary::apt::APT::Keep-Downloaded-Packages "true";
EOF
apt+=(-c "$scratch/keep-downloads.conf")
cp "$sources" "$scratch/ubuntu.sources"
if ! grep -q '^URIs:' "$scratch/ubuntu.sources"; then
  echo 'Expected the Ubuntu archive in the sources.' >&2
  exit 1
fi
case $1 in
  desktop)
    # Keep the hosted runner's Azure-first order and its fallback mirrors.
    apt=(sudo "${apt[@]}")
    ;;
  web)
    # Give the pinned container the runner's Azure archive as well. Retain
    # Ubuntu's suites and keyrings.
    sed -i.orig 's|^URIs: http://archive\.ubuntu\.com/ubuntu/*$|URIs: http://azure.archive.ubuntu.com/ubuntu/|' "$scratch/ubuntu.sources"
    if cmp -s "$scratch/ubuntu.sources.orig" "$scratch/ubuntu.sources"; then
      echo 'Expected Ubuntu archive missing from the web container sources.' >&2
      exit 1
    fi
    ;;
esac
apt+=(-o "Dir::Etc::sourcelist=$scratch/ubuntu.sources" -o Dir::Etc::sourceparts=-)

# Mirror/index/download failures are infrastructure, never a passing check.
if ! "${apt[@]}" -o APT::Update::Error-Mode=any update; then
  echo '::error title=Infrastructure::Ubuntu mirror/index refresh failed.' >&2
  exit 75
fi
if ! "${apt[@]}" install -y --no-install-recommends "${packages[@]}"; then
  echo '::error title=Infrastructure::Ubuntu package provisioning failed.' >&2
  exit 75
fi
