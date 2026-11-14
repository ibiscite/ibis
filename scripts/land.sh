#!/usr/bin/env bash
# The local machines gate a clean commit; GitHub confirms after the squash merge.
# SSH command arguments are deliberately quoted/encoded on the Mac.
# shellcheck disable=SC2029
set -euo pipefail
if [ "${1:-}" = start ] || [ "${1:-}" = end ]; then
  action=$1 name=${2:-} issue=${3:-}
  if [ "$#" -lt 2 ] || { [ "$action" = start ] && [ "$#" -gt 3 ]; } || { [ "$action" = end ] && [ "$#" -ne 2 ]; }; then
    echo 'Usage: land.sh start NAME [ISSUE] | end NAME'; exit 2
  fi
  [[ "$name" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]] && [ "$name" != main ] || { echo 'NAME must be lowercase letters or digits separated by hyphens, and not main.'; exit 2; }
  [[ -z "$issue" || "$issue" =~ ^[0-9]+$ ]] || { echo 'ISSUE must be a number.'; exit 2; }
  root=$(git rev-parse --show-toplevel)
  worktree="$root/../ibis-work/$name"
  if [ "$action" = start ]; then
    title="chore: ${name//-/ }"
    node "$root/scripts/pr-lint.mjs" --title "$title" --branch "$name"
    git fetch -q origin main
    git worktree add -b "$name" "$worktree" origin/main
    (cd "$worktree"; pnpm -C apps/desktop install --frozen-lockfile --offline; git commit --allow-empty -s -m "$title"; git push -u origin "$name")
    body='Handoff: implement the change; next run the focused check; blockers: none.'
    [ -z "$issue" ] || body=$(printf 'Refs #%s\n\n%s' "$issue" "$body")
    gh pr create --draft --base main --head "$name" --title "$title" --body "$body"
  else
    [ -d "$worktree" ] || { echo 'Worktree not found.'; exit 1; }
    status=$(git -C "$worktree" status --porcelain --untracked-files=all)
    [ -z "$status" ] || { echo 'Worktree has uncommitted changes.'; exit 1; }
    current=$(git -C "$worktree" rev-parse --abbrev-ref HEAD)
    head=$(git -C "$worktree" rev-parse HEAD)
    if [ "$current" = "$name" ]; then
      filter=".[] | select(.headRefOid == \"$head\") | .headRefOid"
    elif [ "$current" = main ] && ! git show-ref --verify --quiet "refs/heads/$name"; then
      # gh --delete-branch may already have checked out main and deleted the branch.
      filter='.[0].headRefOid // empty'
    else
      echo 'Worktree branch does not match NAME.'; exit 1
    fi
    [ -n "$(gh pr list --head "$name" --base main --state merged --json headRefOid --jq "$filter")" ] || { echo 'No merged PR for this worktree head.'; exit 1; }
    git worktree remove "$worktree"
    if git show-ref --verify --quiet "refs/heads/$name"; then git branch -D "$name"; fi
  fi
  exit
fi
skip_mac=false skip_ubuntu=false skip_windows=false
title_arg='' body_file=''
while [ "$#" -gt 0 ]; do
  case "$1" in
    --skip)
      case ${2:-} in mac) skip_mac=true;; ubuntu) skip_ubuntu=true;; windows) skip_windows=true;;
        *) echo 'Use --skip mac|ubuntu|windows (repeat to skip more than one).'; exit 2;; esac
      shift 2;;
    --title|--body-file)
      [ "$#" -ge 2 ] || { echo "Missing value for $1"; exit 2; }
      if [ "$1" = --title ]; then title_arg=$2; else body_file=$2; fi
      shift 2;;
    *) echo 'Usage: bash scripts/land.sh [--title TITLE] [--body-file FILE] [--skip MACHINE]'; exit 2;;
  esac
done
branch=$(git rev-parse --abbrev-ref HEAD)
[ "$branch" != main ] && [ "$branch" != HEAD ] || { echo 'Land a named branch, not main or detached HEAD.'; exit 2; }
clean() {
  local status
  status=$(git status --porcelain --untracked-files=all) || return 1
  [ -z "$status" ]
}
clean || { echo 'A clean HEAD is required: commit or remove outstanding work, then rerun.'; exit 2; }
git fetch -q origin main
main=$(git rev-parse origin/main)
if ! git merge-base --is-ancestor "$main" HEAD; then
  git merge --no-edit "$main" ||
    { echo 'Merge conflict with main: resolve it, commit, and rerun.'; exit 1; }
fi
sha=$(git rev-parse HEAD)
unchanged() {
  if [ "$(git rev-parse HEAD)" = "$sha" ] && [ "$(git rev-parse --abbrev-ref HEAD)" = "$branch" ] && clean; then return; fi
  echo 'HEAD or working files changed during landing. Rerun on a clean commit.'; return 1
}
[ "$(gh api user --jq .login)" = diyanko ] || { echo "Landing requires the maintainer's GitHub account."; exit 1; }
repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner)
find_pr() {
  pr=$(gh pr list --repo "$repo" --head "$branch" --base main --state open --json number,author,isCrossRepository \
    --jq '.[0] | if . == null then empty else [.number, .author.login, .isCrossRepository] | @tsv end')
  number=
  if [ -n "$pr" ]; then
    read -r number author fork <<< "$pr"
    case "$author:$fork" in
      diyanko:false|'ibis-release[bot]:false') ;;
      *) echo "Only the maintainer's or release app's in-repository PR can land."; return 1;;
    esac
  fi
}
find_pr
# One checked, NUL-separated list of changed paths drives every path decision; a failed diff stops landing.
paths_file=$(mktemp)
collect_paths() {
  git diff -z --name-only --no-renames "$main...$sha" > "$paths_file" || { echo 'Could not list the changed paths; nothing landed.'; exit 1; }
}
require_review() {
  local path sensitive= comments line
  collect_paths
  # NUL-separated, so no filename is quoted out of a glob.
  while IFS= read -r -d '' path; do
    # Independent-review path table (AGENTS.md). Globs include new files and deletions.
    case "$path" in
      .github/*|scripts/land.sh|scripts/check.mjs|scripts/ci-main.mjs|scripts/ci-result.sh) ;; # CI permissions, what authorizes publication, and the merge boundary
      apps/desktop/src-tauri/src/updater*|scripts/update-feed.mjs) ;;
      scripts/*sign*.sh|scripts/publish-*|scripts/publication-guard.mjs|scripts/keep-previous-nightly.sh) ;;
      apps/desktop/src-tauri/tauri*.json|apps/desktop/src-tauri/capabilities/*|apps/desktop/src-tauri/permissions/*) ;; # Window permissions
      apps/desktop/src-tauri/build.rs|apps/desktop/src-tauri/src/bindings.rs) ;; # IPC permission generation and command registration
      apps/desktop/src/features/picker/updateOfferInput*) ;; # The updater's input-readiness guard
      apps/desktop/src-tauri/src/paste/*|apps/desktop/src-tauri/src/picker/portal.rs) ;; # Native input and the shortcut portal
      apps/desktop/src-tauri/src/editor_endpoint.rs) ;; # Network access
      *) continue;;
    esac
    sensitive+=$'\n'"  $path"
  done < "$paths_file"
  [ -n "$sensitive" ] || return 0
  if [ -n "$number" ]; then
    comments=$(gh api --paginate "repos/$repo/issues/$number/comments" --jq '.[] | select(.user.login == "diyanko") | .body') || comments=
    comments=${comments//$'\r'/}
    while IFS= read -r line; do
      if [[ "$line" =~ ^Reviewed-by-session:\ ([a-zA-Z0-9][a-zA-Z0-9._-]*)\ $sha$ ]] && [ "${BASH_REMATCH[1]}" != "$branch" ]; then return 0; fi
    done <<< "$comments"
  fi
  printf 'Missing independent session review for HEAD %s. Paths:%s\n' "$sha" "$sensitive"
  echo "A session other than '$branch' must review this head and fix every P1/P2, then record:"
  if [ -z "$number" ]; then printf '  gh pr create --draft --repo %q --base main --head %q\n' "$repo" "$branch"; fi
  printf "  gh pr comment %q --repo %q --body 'Reviewed-by-session: <reviewer-session> %s'\n" "${number:-$branch}" "$repo" "$sha"
  return 1
}
commit=$(git rev-list --reverse --no-merges "$main..$sha" | sed -n '1p')
[ -n "$commit" ] || { echo 'No non-merge commits to land.'; exit 2; }
title=${title_arg:-$(git show -s --format=%s "$commit")}
if [ -n "$number" ]; then title=$(gh pr view "$number" --repo "$repo" --json title --jq .title); fi
node scripts/pr-lint.mjs --title "$title" --branch "$branch"
require_review
windows_needed=false
while IFS= read -r -d '' path; do
  case "$path" in
    *.ps1|apps/desktop/src-tauri/packaging/*|apps/desktop/src-tauri/tauri*.json|scripts/*package*|scripts/build-*|scripts/*install*|scripts/*windows*|scripts/*command-path*|scripts/*offline*|scripts/*update*|scripts/land*|scripts/check.mjs|.github/workflows/ci.yml|.github/workflows/preview.yml) windows_needed=true;;
    # Rust compiles differently on Windows (cfg(windows) paths): every Rust change is checked there.
    *.rs|Cargo.toml|*/Cargo.toml|Cargo.lock) windows_needed=true;;
  esac
done < "$paths_file"
root=$(git rev-parse --show-toplevel)
run_id="${sha:0:12}-$(date +%s)-$$"
logs="$root/.local/gates/$run_id"
mkdir -p "$logs"
export CARGO_TARGET_DIR="$root/target"
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=1
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
export PROPTEST_RNG_SEED=20261114
# Empty, not unset: an empty value also overrides a wrapper set in Cargo's config.
export RUSTC_WRAPPER='' RUSTC_WORKSPACE_WRAPPER=''
ssh_options=(-o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=10 -o ServerAliveCountMax=2)

mac_gate() {
  set -Ee
  trap 'printf "FAILED (%s): %s\n" "$?" "$BASH_COMMAND"' ERR
  pnpm -C apps/desktop install --frozen-lockfile --prefer-offline
  node scripts/check.mjs
}

ubuntu_gate() {
  local launch marker attempt misses=0 deadline=$((SECONDS + 3600))
  printf -v launch 'bash -s -- %q %q %q %q' "$sha" "$branch" "$run_id" "$PROPTEST_RNG_SEED"
  # nohup owns the build. Losing this SSH connection cannot lose its result.
  if ! ssh "${ssh_options[@]}" db-scr-ubuntu "$launch" <<'UBUNTU'
set -eu
sha=$1 branch=$2 run_id=$3
export PROPTEST_RNG_SEED=$4
gate="$HOME/Code/ibis/.local/gates/$run_id"
mkdir -p "$(dirname "$gate")"
cat > "$gate.sh" <<'BUILD'
set -Eeuo pipefail
sha=$1 branch=$2
trap 'code=$?; printf "IBIS_GATE_END %s %s\n" "$sha" "$code"' EXIT
trap 'code=$?; printf "FAILED (%s): %s\n" "$code" "$BASH_COMMAND"' ERR
cd "$HOME/Code/ibis"
exec 9>.git/ibis-local-gate.lock
flock -w 3600 9 || { echo 'error: another Ubuntu gate held the checkout for an hour'; exit 1; }
status=$(git status --porcelain --untracked-files=all)
[ -z "$status" ] || { echo 'error: Ubuntu checkout is dirty'; exit 1; }
git fetch -q origin "refs/heads/$branch"
[ "$(git rev-parse FETCH_HEAD)" = "$sha" ] || { echo 'error: fetched branch moved'; exit 1; }
git checkout -q --detach "$sha"
. "$HOME/.cargo/env"
export NVM_DIR="$HOME/.nvm"
. "$NVM_DIR/nvm.sh"
nvm use 26
export CARGO_TARGET_DIR="$HOME/Code/ibis/target"
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=1 CARGO_BUILD_JOBS=4  # 8 cores but 7 GB RAM
# Empty, not unset: an empty value also overrides a wrapper set in Cargo's config.
export RUSTC_WRAPPER= RUSTC_WORKSPACE_WRAPPER=
pnpm -C apps/desktop install --frozen-lockfile --prefer-offline
node scripts/check.mjs rust
node scripts/check.mjs vitest
BUILD
nohup bash "$gate.sh" "$sha" "$branch" > "$gate.log" 2>&1 < /dev/null &
echo "Ubuntu started: $!"
UBUNTU
  then echo 'Ubuntu launch connection lost; checking the detached log.'; fi
  while [ "$SECONDS" -lt "$deadline" ]; do
    if marker=$(ssh "${ssh_options[@]}" db-scr-ubuntu "tail -n 1 \"\$HOME/Code/ibis/.local/gates/$run_id.log\""); then
      misses=0
      case "$marker" in
        "IBIS_GATE_END $sha "*)
          for attempt in 1 2 3; do
            if ssh "${ssh_options[@]}" db-scr-ubuntu "cat \"\$HOME/Code/ibis/.local/gates/$run_id.log\""; then
              [ "$marker" = "IBIS_GATE_END $sha 0" ]; return
            fi
            [ "$attempt" = 3 ] || sleep 5
          done
          echo 'Ubuntu unreachable while retrieving its completed log.'; return 1;;
        IBIS_GATE_END*) echo 'error: Ubuntu returned a result for another SHA'; return 1;;
      esac
    else
      misses=$((misses + 1))
      [ "$misses" -lt 3 ] || { echo 'Ubuntu unreachable or detached log unavailable (three attempts).'; return 1; }
    fi
    sleep 5
  done
  echo 'error: Ubuntu gate timed out after one hour; its detached log remains on Ubuntu.'
  return 1
}

windows_gate() {
  local encoded
  # JavaScript builds a PowerShell script; its dollar signs stay literal here.
  # shellcheck disable=SC2016
  encoded=$(node --input-type=module -e '
    import { readFileSync } from "node:fs";
    const quote = (s) => "\x27" + s.replaceAll("\x27", "\x27\x27") + "\x27";
    const [sha, branch, repo] = process.argv.slice(1);
    const script = `$sha = ${quote(sha)}\n$branch = ${quote(branch)}\n$repo = ${quote(repo)}\n` + readFileSync(0, "utf8");
    process.stdout.write(Buffer.from(script, "utf16le").toString("base64"));
  ' "$sha" "$branch" "$repo" <<'WINDOWS'
$ErrorActionPreference = 'Stop'
# Over ssh the laptop's PATH lacks Windows PowerShell; tests and scripts expect it there.
$env:Path = "$env:USERPROFILE\miniconda3;$env:USERPROFILE\miniconda3\Scripts;$env:SystemRoot\System32\WindowsPowerShell\v1.0;$env:Path"
$checkout = Join-Path $env:TEMP 'ibis-local-gate'
$lock = [IO.File]::Open("$checkout.lock", 'OpenOrCreate', 'ReadWrite', 'None')
try {
  if (-not (Test-Path "$checkout/.git")) {
    git clone "https://github.com/$repo.git" $checkout
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  }
  Set-Location $checkout
  $dirty = git status --porcelain --untracked-files=all
  if ($LASTEXITCODE -ne 0 -or $dirty) { throw 'Windows checkout is unavailable or dirty' }
  git fetch -q origin "refs/heads/$branch"
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  $fetched = git rev-parse FETCH_HEAD
  if ($LASTEXITCODE -ne 0 -or $fetched -ne $sha) { throw 'Fetched branch moved' }
  git checkout -q --detach $sha
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  node scripts/check.mjs windows
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  # The laptop's warm build cache; gates run one at a time.
  $env:CARGO_TARGET_DIR = Join-Path $env:USERPROFILE 'Code\ibis\target'
  $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
  node scripts/check.mjs windows-rust
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally { $lock.Dispose() }
WINDOWS
  )
  ssh "${ssh_options[@]}" diyanko@diyanko-predator.elephant-halfmoon.ts.net \
    "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand $encoded"
}

unchanged
git push -q -u origin "$sha:refs/heads/$branch"
echo "Checking $sha in parallel. Logs: $logs"
mac='skipped explicitly' ubuntu='skipped explicitly' windows='not needed'
pids=() machines=()
if ! "$skip_mac"; then mac_gate > "$logs/mac.log" 2>&1 & pids+=("$!"); machines+=(mac); fi
if ! "$skip_ubuntu"; then ubuntu_gate > "$logs/ubuntu.log" 2>&1 & pids+=("$!"); machines+=(ubuntu); fi
if "$skip_windows"; then windows='skipped explicitly'
elif "$windows_needed"; then windows_gate > "$logs/windows.log" 2>&1 & pids+=("$!"); machines+=(windows); fi
failed=false
for i in "${!pids[@]}"; do
  machine=${machines[$i]}
  if wait "${pids[$i]}"; then
    printf -v "$machine" '%s' passed
    echo "$machine passed."
  else
    failed=true
    echo "$machine failed (PROPTEST_RNG_SEED=$PROPTEST_RNG_SEED):"
    grep -m 8 -A 3 -Ei 'FAILED|FAIL |error([ :\[]|$)|unreachable|fatal:|Permission denied|not found|Exception' "$logs/$machine.log" || head -n 12 "$logs/$machine.log"
  fi
done
"$failed" && { echo "Gate failed. Full logs: $logs"; exit 1; }
unchanged
find_pr
require_review
if [ -n "$number" ]; then
  current_title=$(gh pr view "$number" --repo "$repo" --json title --jq .title)
  node scripts/pr-lint.mjs --title "$current_title" --branch "$branch"
  [ "$(gh pr view "$number" --repo "$repo" --json headRefOid --jq .headRefOid)" = "$sha" ] || { echo 'PR head moved; rerun.'; exit 1; }
  gh pr view "$number" --repo "$repo" --json body --jq .body > "$logs/body.md"
elif [ -n "$body_file" ]; then cp "$body_file" "$logs/body.md"
else
  git show -s --format=%b "$commit" | node --input-type=module -e '
    import { readFileSync } from "node:fs";
    const paragraphs = readFileSync(0, "utf8").trimEnd().split(/\r?\n\r?\n/);
    if (/^[A-Za-z0-9-]+:/.test(paragraphs.at(-1) ?? "") && paragraphs.at(-1).split(/\r?\n/).every((line) => /^[A-Za-z0-9-]+:|^[ \t]/.test(line))) paragraphs.pop();
    console.log(paragraphs.join("\n\n"));
  ' > "$logs/body.md"
fi
summary="mac $mac; ubuntu $ubuntu; windows $windows"
# Markdown's backticks are literal.
# shellcheck disable=SC2016
printf '\nLocal gate `%s`: %s.\n' "$sha" "$summary" >> "$logs/body.md"
trailers=$(git log --format='%(trailers:key=Signed-off-by,key=Assisted-by,only,unfold)' "$main..$sha" | awk 'NF && !seen[$0]++')
if [ -n "$trailers" ]; then printf '\n%s\n' "$trailers" >> "$logs/body.md"; fi
unchanged
if [ -n "$number" ]; then
  gh pr edit "$number" --repo "$repo" --body-file "$logs/body.md"
  url=$(gh pr view "$number" --repo "$repo" --json url --jq .url)
else
  url=$(gh pr create --repo "$repo" --base main --head "$branch" --title "$title" --body-file "$logs/body.md")
fi
git fetch -q origin main
[ "$(git rev-parse origin/main)" = "$main" ] || { echo 'main moved; rerun'; exit 1; }
unchanged
gh api "repos/$repo/statuses/$sha" -f 'context=Local gate' -f state=success -f "description=$summary" > /dev/null
gh pr merge "$url" --repo "$repo" --squash --body-file "$logs/body.md" --match-head-commit "$sha" --delete-branch
printf 'Merged %s (%s).\n' "$url" "$summary"
