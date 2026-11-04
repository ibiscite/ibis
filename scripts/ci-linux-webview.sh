#!/usr/bin/env bash
# CI's "Install system WebView libraries (Linux)" step (ci.yml).
# Usage: scripts/ci-linux-webview.sh [mirror-list]
# The mirror list defaults to the runner's /etc/apt/apt-mirrors.txt.
set -e
mirrors=${1:-/etc/apt/apt-mirrors.txt}
# Prefer the HTTPS archive already configured by runner-images (#857).
sudo python3 - "$mirrors" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
archive = "https://archive.ubuntu.com/ubuntu/"
lines = path.read_text().splitlines(keepends=True)
matches = [i for i, line in enumerate(lines) if line.split()[:1] == [archive]]
if len(matches) != 1 or lines[matches[0]].split() not in (
    [archive, "priority:2"], [archive, "priority:0"]
):
    raise SystemExit("expected runner archive mirror is missing or ambiguous")
lines[matches[0]] = f"{archive}\tpriority:0\n"
path.write_text("".join(lines))
PY
# Match runner-images' acquire bounds even on images with older defaults (#857).
ibis_apt_options=(-o Acquire::Retries=1 -o Acquire::http::Timeout=15 -o Acquire::https::Timeout=15)
sudo apt-get "${ibis_apt_options[@]}" update
sudo apt-get "${ibis_apt_options[@]}" install -y --no-install-recommends libwebkit2gtk-4.1-dev librsvg2-dev librsvg2-bin fonts-inter libsoup-3.0-dev libjavascriptcoregtk-4.1-dev patchelf xvfb xauth imagemagick fonts-noto-core fonts-noto-cjk xdotool x11-utils openbox xterm python3-pyatspi xclip at-spi2-core iproute2 lsof
