#!/usr/bin/env bash
# Systems for Test (core tests; Desktop runs on all three). A pull request runs Ubuntu only unless it has the
# full-ci label or changes platform code, packaging, dependencies or workflows;
# every other event, and any unreadable diff, runs all three.
set -euo pipefail
all='["ubuntu-24.04","macos-15","windows-2025"]'
platform='(^|/)[^/]*(macos|windows|darwin|win32)[^/]*($|/)|(^|/)platform/|\.(ps1|swift|nsh|wxs|plist)$'
platform+='|^apps/desktop/src-tauri/(src/(paste|copy|placement|native_feel)/|packaging/|build\.rs$|Cargo\.toml$|tauri[^/]*\.json$)'
platform+='|^(Cargo\.(toml|lock)|rust-toolchain\.toml|\.cargo/.*|\.github/workflows/.*)$'
if [ "${EVENT:-}" != pull_request ] || [ "${FULL_LABEL:-}" = true ]; then echo "os=$all"; exit; fi
# A pull request's checkout is GitHub's merge commit; its first parent is main.
files=$(git diff --name-only HEAD^1 HEAD) || { echo "os=$all"; exit; }
if matched=$(grep -E "$platform" <<< "$files"); then
  printf 'All systems for:\n%s\n' "$matched" >&2
  echo "os=$all"
else
  echo 'os=["ubuntu-24.04"]'
fi
