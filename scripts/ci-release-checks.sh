#!/usr/bin/env bash
# Release-mode, minimum-Rust and performance checks for workspace changes and releases.
set -euo pipefail
cargo test -p ibis-core -p ibis-sources --release --locked
rustup toolchain install 1.90.0 --profile minimal
cargo +1.90.0 test --locked
# A pull request's checkout is its merge into the base, so the base commit is
# the one it would change: compare against it on this runner. Everything else
# meets the shared-runner budgets (docs/architecture.md › Performance checks).
if [ "${GITHUB_EVENT_NAME:-}" = pull_request ]; then
  base=$(node -p 'require(process.env.GITHUB_EVENT_PATH).pull_request.base.sha')
  git fetch --no-tags --depth=1 origin "$base"
  node scripts/perf.mjs compare "$base"
else
  node scripts/perf.mjs check shared
fi
