#!/usr/bin/env bash
# Preview's "Build the web interface" step for the Windows paste check
# (preview.yml), from the repository root. A failed install stops the build.
set -eo pipefail
cd apps/desktop
pnpm install --frozen-lockfile
pnpm exec vite build
