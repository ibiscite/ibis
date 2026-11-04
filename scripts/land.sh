#!/usr/bin/env bash
# Land the current branch's pull request the way AGENTS.md › Merging asks,
# in one command: merge origin/main, check what the change can break, push,
# and turn on squash auto-merge. Stops at the first problem.
#   bash scripts/land.sh            # full: workspace compile + desktop suite when Rust changed
#   bash scripts/land.sh --quick    # compile only
set -euo pipefail
quick=${1:-}
branch=$(git rev-parse --abbrev-ref HEAD)
[ "$branch" != main ] || { echo "On main: land a branch, not main."; exit 2; }
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$(git rev-parse --show-toplevel)/target}

git fetch -q origin main "$branch" 2>/dev/null || git fetch -q origin main
git merge -q --no-edit origin/main || { echo "Merge conflict with main: resolve it, commit, and rerun."; exit 1; }

changed=$(git diff --name-only origin/main...HEAD)
rust=$(grep -E '\.rs$|Cargo\.(toml|lock)$' <<<"$changed" || true)
web=$(grep -E '^apps/desktop/(src|canvas)/' <<<"$changed" || true)
scripts=$(grep -E '^(scripts|website)/' <<<"$changed" || true)

if [ -n "$rust" ]; then
  cargo fmt --all --check
  (cd apps/desktop/src-tauri && cargo fmt --check)
  # A merge with no conflict can still fail to compile: build every test target.
  cargo test --workspace --locked --no-run -q
  [ "$quick" = --quick ] || cargo test -p ibis-desktop --locked -q
fi
if [ -n "$web" ]; then
  (cd apps/desktop && pnpm install --frozen-lockfile >/dev/null && pnpm exec tsc --noEmit -p . && pnpm exec biome lint src canvas)
  files=$(grep -E '\.test\.tsx?$' <<<"$web" | sed 's|^apps/desktop/||' || true)
  [ -z "$files" ] || (cd apps/desktop && pnpm exec vitest run $files --maxWorkers=1)
  (cd apps/desktop && pnpm exec vitest run src/design.test.ts src/ui/ownership.test.ts --maxWorkers=1)
fi
[ -z "$scripts" ] || node --test scripts/*.test.mjs

git push -q -u origin "$branch"
pr=$(gh api "repos/{owner}/{repo}/pulls?head={owner}:$branch&state=open" --jq '.[0].number' 2>/dev/null || true)
if [ -n "$pr" ] && [ "$pr" != null ]; then
  gh api -X PUT "repos/{owner}/{repo}/pulls/$pr/ccr/auto_merge" -f merge_method=squash >/dev/null && echo "Pushed; #$pr merges itself on a green CI result."
else
  echo "Pushed $branch. Open its pull request, then rerun to turn on auto-merge."
fi
