#!/usr/bin/env bash
# Heavy Linux qualification, shared by daily/manual CI and numbered releases.
set -euo pipefail
cargo test -p ibis-core -p ibis-sources --release --locked
rustup toolchain install 1.90.0 --profile minimal
cargo +1.90.0 test --locked
node scripts/preview-budgets.mjs
cargo test -p ibis-engine --locked --release --test refresh_budget -- search_budget --ignored --nocapture --test-threads=1
