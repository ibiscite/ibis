#!/usr/bin/env bash
# Scheduled desktop Vitest passes. An optional count uses the first N seeds.
set -euo pipefail
seeds=(1 2 3 4 5 6 7 8 9 10)
passes=${1:-${#seeds[@]}}
if [[ ! $passes =~ ^([1-9]|10)$ ]]; then
  echo 'Usage: bash scripts/ci-flake-hunt.sh [passes (1–10)]' >&2
  exit 2
fi

failed=()
for seed in "${seeds[@]:0:passes}"; do
  echo "=== Desktop Vitest shuffle seed $seed ==="
  # Vitest's default reporter names failed tests. Disable retries so a test
  # that passes on retry still counts as a failure on this seed.
  if ! pnpm -C apps/desktop exec vitest run --sequence.shuffle --sequence.seed "$seed" --maxWorkers=2 --retry=0; then
    failed+=("$seed")
  fi
done
if (( ${#failed[@]} )); then
  echo "Desktop Vitest shuffle failed on seeds: ${failed[*]} (see the failed tests under each seed above)." >&2
  exit 1
fi
echo "Desktop Vitest passed on all $passes shuffle seeds."
