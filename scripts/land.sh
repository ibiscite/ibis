#!/usr/bin/env bash
# Land the current branch's pull request the way AGENTS.md › Merging asks,
# in one command: refuse a conflict with main, check what the change can
# break, push, and turn on squash auto-merge. Stops at the first problem.
#   bash scripts/land.sh
# The checks follow `node scripts/check.mjs --plan`, the classes CI picks for
# this change: native changes type check every target, interface changes type
# check and lint. Test suites are CI's job, apart from the changed interface
# and script tests and the tests of changed scripts.
set -euo pipefail
branch=$(git rev-parse --abbrev-ref HEAD)
[ "$branch" != main ] && [ "$branch" != HEAD ] || { echo "Land a named branch, not main or detached HEAD."; exit 2; }
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$(git rev-parse --show-toplevel)/target}

git fetch -q origin main "$branch" 2>/dev/null || git fetch -q origin main
# A conflict is the one case where GitHub runs no CI. Otherwise the pull
# request's CI and the merge queue test the change on main, so main is not
# merged here: that would rebuild other lanes' work before every push.
git merge-tree --write-tree origin/main HEAD >/dev/null ||
  { echo "Merge conflict with main: merge origin/main, resolve it, commit, and rerun."; exit 1; }

plan=" $(node scripts/check.mjs --plan) "
changed=$(git diff --name-only origin/main...HEAD)
web=$(grep -E '^apps/desktop/(src|canvas)/' <<<"$changed" || true)
scripts=$(grep -E '^(scripts|website)/' <<<"$changed" || true)

if [[ "$plan" == *" native "* ]]; then
  # The desktop crate is a workspace member, so one check covers it.
  cargo fmt --all --check
  # Type check every target, tests included; CI links and runs them.
  cargo check --workspace --all-targets --locked -q
fi
if [[ "$plan" == *" interface "* ]]; then
  (cd apps/desktop && pnpm install --frozen-lockfile >/dev/null && pnpm exec tsc --noEmit -p . && pnpm exec biome lint src canvas)
  files=$(grep -E '\.test\.tsx?$' <<<"$web" | sed 's|^apps/desktop/||' || true)
  [ -z "$files" ] || (cd apps/desktop && pnpm exec vitest run $files --maxWorkers=1)
  (cd apps/desktop && pnpm exec vitest run src/design.test.ts src/ui/ownership.test.ts --maxWorkers=1)
fi
# The changed script tests, and the test of each changed script.
tests=$(for file in $scripts; do
  case "$file" in
    (scripts/*.test.mjs) test="$file" ;;
    (scripts/*.mjs | scripts/*.sh) test="${file%.*}.test.mjs" ;;
    (*) continue ;;
  esac
  if [ -f "$test" ]; then echo "$test"; fi
done | sort -u)
[ -z "$tests" ] || node --test $tests

node scripts/check-agent-identity.mjs
git push -q -u origin "$branch"
pr=$(gh api "repos/{owner}/{repo}/pulls?head={owner}:$branch&state=open" --jq '.[0].number')
if [ -n "$pr" ] && [ "$pr" != null ]; then
  gh pr merge "$pr" --squash --auto && echo "Pushed; #$pr joins the merge queue once its CI is green."
else
  echo "Pushed $branch. Open its pull request, then rerun to turn on auto-merge."
fi
