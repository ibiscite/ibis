#!/usr/bin/env bash
# Land the current branch's pull request the way AGENTS.md › Merging asks,
# in one command: merge origin/main, check what the merge can break, push,
# and turn on squash auto-merge. Stops at the first problem.
#   bash scripts/land.sh
# The checks follow `node scripts/check.mjs --plan`, the classes CI picks for
# this change: native changes compile every test target, interface changes
# type check and lint. Test suites are CI's job, apart from changed interface
# tests and, for script or website changes, the script tests.
set -euo pipefail
branch=$(git rev-parse --abbrev-ref HEAD)
[ "$branch" != main ] && [ "$branch" != HEAD ] || { echo "Land a named branch, not main or detached HEAD."; exit 2; }
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$(git rev-parse --show-toplevel)/target}

git fetch -q origin main "$branch" 2>/dev/null || git fetch -q origin main
git merge -q --no-commit origin/main || { echo "Merge conflict with main: resolve it, commit, and rerun."; exit 1; }
if git rev-parse -q --verify MERGE_HEAD >/dev/null; then
  git commit -s --no-edit --trailer "Assisted-by: Ibis development tooling (AI-assisted)" >/dev/null
fi

plan=" $(node scripts/check.mjs --plan) "
changed=$(git diff --name-only origin/main...HEAD)
web=$(grep -E '^apps/desktop/(src|canvas)/' <<<"$changed" || true)
scripts=$(grep -E '^(scripts|website)/' <<<"$changed" || true)

if [[ "$plan" == *" native "* ]]; then
  cargo fmt --all --check
  (cd apps/desktop/src-tauri && cargo fmt --check)
  # A merge with no conflict can still fail to compile: build every test target.
  cargo test --workspace --locked --no-run -q
fi
if [[ "$plan" == *" interface "* ]]; then
  (cd apps/desktop && pnpm install --frozen-lockfile >/dev/null && pnpm exec tsc --noEmit -p . && pnpm exec biome lint src canvas)
  files=$(grep -E '\.test\.tsx?$' <<<"$web" | sed 's|^apps/desktop/||' || true)
  [ -z "$files" ] || (cd apps/desktop && pnpm exec vitest run $files --maxWorkers=1)
  (cd apps/desktop && pnpm exec vitest run src/design.test.ts src/ui/ownership.test.ts --maxWorkers=1)
fi
[ -z "$scripts" ] || node --test scripts/*.test.mjs

node scripts/check-agent-identity.mjs
git push -q -u origin "$branch"
pr=$(gh api "repos/{owner}/{repo}/pulls?head={owner}:$branch&state=open" --jq '.[0].number')
if [ -n "$pr" ] && [ "$pr" != null ]; then
  gh pr merge "$pr" --squash --auto && echo "Pushed; #$pr joins the merge queue once its CI is green."
else
  echo "Pushed $branch. Open its pull request, then rerun to turn on auto-merge."
fi
