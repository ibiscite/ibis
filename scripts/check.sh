#!/usr/bin/env bash
# The checks CI runs, by part, in one place: CI's jobs call these parts and a
# machine runs the same ones before it pushes.
#   bash scripts/check.sh           the parts this branch can affect, as CI plans them
#   bash scripts/check.sh all       every part
#   bash scripts/check.sh PART...   named parts
# CI alone runs the picture gate (pinned Linux WebKit), the other systems,
# the native GUI tests, actionlint, cargo deny, the release checks and the
# daily qualification; the ci:full label has no effect here.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
python=python3
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) python=python;; esac

part_identities() { node scripts/check-agent-identity.mjs; }
part_scripts() {
  node --test --test-concurrency=2 scripts/*.test.mjs
  for test in scripts/test_*.py scripts/*.test.py; do "$python" -B "$test"; done
}
part_build() { (cd apps/desktop && pnpm exec vite build && node ../../scripts/picker-budget.mjs dist); }
part_interface() {
  (cd apps/desktop && pnpm typecheck && pnpm lint && pnpm test)
  part_build
}
part_format() { cargo fmt --all --check; }
part_brand() { "$python" brand/build.py --check; }
# Warnings fail here as they do on CI (RUSTFLAGS there), without a rebuild.
part_lint() { cargo clippy --all-targets --locked -- -D warnings; }
part_lint_app() { cargo clippy -p ibis-desktop -p ibis-desktop-macros --all-targets --locked -- -D warnings; }
part_test() { cargo test --locked; }
part_test_app() {
  cargo test -p ibis-desktop -p ibis-desktop-macros --locked
  # Embed the built interface; a plain debug build uses localhost:5173.
  cargo build -p ibis-desktop --locked --features tauri/custom-protocol
}
# Needs Pandoc: without it the comparisons would be skipped and still pass.
part_fidelity() { IBIS_REQUIRE_PANDOC=1 cargo test -p ibis --test fidelity --locked; }
part_generated() { node scripts/gen.mjs --check; }
part_csl() {
  node scripts/csl-suite.mjs
  cargo test --locked -p ibis-csl --lib conformance -- --ignored
}

ALL=(identities scripts interface format brand lint lint-app test test-app fidelity generated csl)
if [ "$#" -eq 0 ]; then
  plan=$(node scripts/ci-plan.mjs --local)
  echo "$plan"
  parts=(identities scripts interface)
  case "$plan" in *rust=true*) parts+=(format);; esac
  case "$plan" in *rust=true*) [ "$(uname -s)" != Linux ] || parts+=(brand);; esac
  case "$plan" in *workspace=true*) parts+=(lint);; esac
  case "$plan" in *rust=true*) parts+=(lint-app);; esac
  case "$plan" in *workspace=true*) parts+=(test);; esac
  case "$plan" in *rust=true*) parts+=(test-app);; esac
  # Linux only, as on CI, where Pandoc is installed for it.
  case "$plan" in *workspace=true*) [ "$(uname -s)" != Linux ] || parts+=(fidelity);; esac
  case "$plan" in *rust=true*) parts+=(generated);; esac
  case "$plan" in *csl=true*) parts+=(csl);; esac
  case "$plan" in *pictures=true*) echo "The picture gate runs on CI; render locally with: pnpm -C apps/desktop canvas";; esac
elif [ "$*" = all ]; then
  parts=("${ALL[@]}")
else
  parts=("$@")
fi
for part in "${parts[@]}"; do
  declare -F "part_${part//-/_}" >/dev/null || { echo "Unknown part: $part (parts: build ${ALL[*]})"; exit 2; }
  printf '\n== %s\n' "$part"
  "part_${part//-/_}"
done
printf '\nPassed: %s\n' "${parts[*]}"
