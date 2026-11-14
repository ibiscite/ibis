import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const sha = "a".repeat(40), posix = { skip: process.platform === "win32" };
function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), "ibis-land-")), bin = join(root, "bin"), calls = join(root, "calls");
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const path of ["bin", "scripts", "checkout/scripts", "ubuntu/Code/ibis/.git", "ubuntu/.cargo", "ubuntu/.nvm"]) mkdirSync(join(root, path), { recursive: true });
  writeFileSync(join(root, "ubuntu/.cargo/env"), "");
  writeFileSync(join(root, "ubuntu/.nvm/nvm.sh"), "nvm() { return 0; }\n");
  writeFileSync(join(root, "scripts/check-synthetic.mjs"), "");
  copyFileSync(fileURLToPath(new URL("./pr-lint.mjs", import.meta.url)), join(root, "scripts/pr-lint.mjs"));
  copyFileSync(fileURLToPath(new URL("./pr-lint.mjs", import.meta.url)), join(root, "checkout/scripts/pr-lint.mjs"));
  writeFileSync(join(root, "scripts/synthetic.test.mjs"), "");
  writeFileSync(join(root, "supplied-body"), "Caller supplied body.\n");
  for (const [name, body] of Object.entries({
    git: `if [ "$1" = -C ]; then shift 2; fi
    case "$1" in
      rev-parse) case "$2" in --abbrev-ref) echo "$BRANCH";; --show-toplevel) echo "$ROOT/checkout";; FETCH_HEAD) echo "$FETCHED_SHA";; *) if [ "$MACHINE" = mac ] && [ -f "$ROOT/changed" ]; then echo bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb; else echo "$SHA"; fi;; esac;;
      status) [ "$STATUS_ERROR" = 0 ] || exit "$STATUS_ERROR"; if [ "$DIRTY" = 1 ]; then echo ' M synthetic'; fi;;
      merge-tree) exit "$CONFLICT";;
      rev-list) echo "$SHA";;
      show) if [ "$3" = --format=%s ]; then echo 'ci: simplify the pipeline'; else printf 'Synthetic change.\\n\\nCloses #7\\n\\nSigned-off-by: Synthetic <synthetic@example.org>\\nAssisted-by: Codex\\n'; fi;;
      log) printf 'Signed-off-by: Synthetic <synthetic@example.org>\\nAssisted-by: Codex\\n';;
      show-ref) [ "$BRANCH_EXISTS" = 1 ];;
      worktree) if [ "$2" = add ]; then mkdir -p "$5"; else rm -rf "$3"; fi;;
      diff) case " $* " in *" -z "*) printf '%s\\0' "$FILES";; *) printf '%s\\n' "$FILES";; esac;;
      push) exit "$PUSH_STATUS";;
    esac`,
    cargo: `if [ "$1" = clippy ]; then
      touch "$ROOT/$MACHINE-started"
      if [ "$PARALLEL" = 1 ]; then
        other=ubuntu; [ "$MACHINE" != ubuntu ] || other=mac
        for ((n=0; n<200; n++)); do [ ! -f "$ROOT/$other-started" ] || break; sleep 0.01; done
        [ -f "$ROOT/$other-started" ] || { echo 'error: gates did not overlap'; exit 1; }
      fi
      if [ "$MACHINE" = "$FAIL_MACHINE" ]; then echo "error: synthetic $MACHINE failure"; exit 1; fi
    fi
    if [ "$1" = test ]; then
      printf 'seed [%s] %s\\n' "$MACHINE" "$PROPTEST_RNG_SEED" >> "$CALLS"
      if [ "$MACHINE" = "$FAIL_TEST_MACHINE" ]; then echo "error: synthetic property failure"; exit 1; fi
    fi`,
    pnpm: `exit 0`,
    node: `case "$1" in
      --input-type=module) exec "$REAL_NODE" "$@";;
      *pr-lint.mjs) exec "$REAL_NODE" "$@";;
      --test) if [ "$MACHINE" = mac ] && [ "$MOVE_HEAD" = 1 ]; then touch "$ROOT/changed"; fi;;
    esac`,
    sleep: `if [ "$1" = 5 ]; then exec /bin/sleep 0.01; else exec /bin/sleep "$@"; fi`,
    flock: `[ "$LOCK_BUSY" = 0 ]`,
    ssh: `while [ "$#" -gt 0 ]; do case "$1" in -o) shift 2;; *) break;; esac; done
      host=$1; shift
      if [ "$host" = db-scr-ubuntu ]; then
        [ "$UNREACHABLE" != ubuntu ] || { echo 'ssh: Ubuntu unreachable' >&2; exit 255; }
        if [ "$DROP_POLL" = 1 ] && [ -f "$ROOT/launched" ] && [ ! -f "$ROOT/dropped" ]; then touch "$ROOT/dropped"; echo 'ssh: connection reset' >&2; exit 255; fi
        if [ "$DROP_LOG" = 1 ] && [[ "$*" == cat* ]] && [ ! -f "$ROOT/dropped-log" ]; then touch "$ROOT/dropped-log"; exit 255; fi
        if [[ "$*" == *'bash -s --'* ]]; then touch "$ROOT/launched"; fi
        export HOME="$ROOT/ubuntu" MACHINE=ubuntu
        unset PROPTEST_RNG_SEED
        if [ "$DROP_LAUNCH" = 1 ] && [[ "$*" == *'bash -s --'* ]]; then bash -c "$*"; exit 255; fi
        exec bash -c "$*"
      else
        cat > "$ROOT/windows-input"
        [ "$UNREACHABLE" != windows ] || { echo 'ssh: Windows unreachable' >&2; exit 255; }
        [ "$FAIL_MACHINE" != windows ] || { echo 'Error: synthetic Windows failure'; exit 1; }
      fi`,
    gh: `case "$1 $2" in
      'api user') echo "$GH_ACTOR";;
      'api --paginate')
        if [ "$REVIEW_ERROR" = 1 ]; then exit 1; fi
        if [ "$DROP_REVIEW" = 1 ] && [ -f "$ROOT/mac-started" ]; then exit 0; fi
        if [ "$REVIEW_AUTHOR" = diyanko ]; then printf '%s\\n' "$REVIEW"; fi;;
      'repo view') echo example/ibis;;
      'pr list') if [[ "$*" == *'--state merged'* ]]; then [ "$MERGED" = 0 ] || echo "$SHA"; elif [ "$HAS_PR" = 1 ]; then printf '42\\t%s\\t%s\\n' "$AUTHOR" "$FORK"; fi;;
      'pr view') case "$*" in *--json\\ title*) echo "$PR_TITLE";; *--json\\ body*) printf 'Reviewed body.\\n\\n## Handoff\\nExisting instructions.\\n';; *--json\\ headRefOid*) echo "$PR_SHA";; *) echo https://example.invalid/pull/42;; esac;;
      'pr create'|'pr edit') while [ "$#" -gt 0 ]; do if [ "$1" = --body-file ]; then cp "$2" "$ROOT/body"; fi; shift; done; echo https://example.invalid/pull/42;;
      'api repos/'*) exit "$STATUS_STATUS";;
      'pr merge') exit "$MERGE_STATUS";;
    esac`,
  })) writeFileSync(join(bin, name), `#!/usr/bin/env bash\nprintf '${name} [%s] %s\\n' "$MACHINE" "$*" >> "$CALLS"\n${body}\n`, { mode: 0o755 });
  return (overrides = {}, args = []) => {
    for (const path of ["changed", "body", "mac-started", "ubuntu-started", "launched", "dropped", "dropped-log"]) rmSync(join(root, path), { force: true });
    writeFileSync(calls, "");
    const result = spawnSync("bash", [fileURLToPath(new URL("./land.sh", import.meta.url)), ...args], { cwd: root, encoding: "utf8", timeout: 15000,
      env: { ...process.env, PATH: `${bin}${delimiter}${process.env.PATH}`, ROOT: root, CALLS: calls, REAL_NODE: process.execPath, MACHINE: "mac", BRANCH: "ci/synthetic", BRANCH_EXISTS: "1", SHA: sha, PR_SHA: sha, PR_TITLE: "fix: preserve the reviewed title", FETCHED_SHA: sha, DIRTY: "0", MOVE_HEAD: "0", STATUS_ERROR: "0", GH_ACTOR: "diyanko", CONFLICT: "0", PUSH_STATUS: "0", HAS_PR: "1", MERGED: "1", AUTHOR: "diyanko", FORK: "false", MERGE_STATUS: "0", STATUS_STATUS: "0", FAIL_MACHINE: "", FAIL_TEST_MACHINE: "", UNREACHABLE: "", LOCK_BUSY: "0", DROP_POLL: "0", DROP_LAUNCH: "0", DROP_LOG: "0", REVIEW: `Reviewed-by-session: independent ${sha}`, REVIEW_AUTHOR: "diyanko", REVIEW_ERROR: "0", DROP_REVIEW: "0", FILES: "docs/testing.md", PARALLEL: "1", ...overrides } });
    let body = ""; try { body = readFileSync(join(root, "body"), "utf8"); } catch {}
    return { ...result, calls: readFileSync(calls, "utf8"), body };
  };
}

test("parallel gates set Local gate on the checked SHA and immediately squash merge", posix, (t) => {
  const result = fixture(t)({ HAS_PR: "0", FILES: "scripts/build-windows-packages.sh", DROP_POLL: "1", DROP_LAUNCH: "1", DROP_LOG: "1" });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.calls, /cargo \[mac\] clippy --workspace --all-targets --locked/);
  assert.match(result.calls, /cargo \[ubuntu\] test --workspace --locked/);
  assert.match(result.calls, /pnpm \[mac\].*tsc --noEmit/);
  assert.match(result.calls, /node \[mac\] --test --test-concurrency=2 scripts\/synthetic.test.mjs/);
  assert.match(result.calls, /node \[mac\] scripts\/gen.mjs --headless --check/);
  assert.match(result.calls, new RegExp(`gh \\[mac\\] api repos/example/ibis/statuses/${sha} .*context=Local gate.*state=success`));
  assert.match(result.calls, new RegExp(`gh \\[mac\\] pr merge .*--squash --body-file .* --match-head-commit ${sha}`));
  assert.doesNotMatch(result.calls, /--auto|--admin|target_url/);
  assert.ok(result.calls.indexOf("git [mac] push") < result.calls.indexOf("cargo [mac] clippy"));
  assert.ok(result.calls.indexOf("cargo [ubuntu] test") < result.calls.indexOf("gh [mac] pr create"));
  assert.match(result.body, /Synthetic change\.[\s\S]*Closes #7[\s\S]*Local gate.*mac passed; ubuntu passed/);
  assert.match(result.body, /Signed-off-by: Synthetic <synthetic@example.org>\nAssisted-by: Codex/);
  assert.match(result.calls, /pr merge .*--delete-branch/);
  const windows = Buffer.from(result.calls.match(/-EncodedCommand ([A-Za-z0-9+/=]+)/)[1], "base64").toString("utf16le");
  assert.match(windows, /PSVersionTable\.PSVersion\.Major -ne 5/);
  assert.match(windows, /powershell\.exe .* -File \$test.FullName/);
  assert.match(result.body, /windows passed/);
});

test("security paths require an independent session record on the exact head", posix, (t) => {
  const run = fixture(t), path = "apps/desktop/src-tauri/src/updater.rs";
  for (const env of [{ REVIEW: "" }, { REVIEW: `Reviewed-by-session: independent ${"b".repeat(40)}` },
    { REVIEW: `Reviewed-by-session: independent ${sha.slice(0, 12)}` },
    { REVIEW_AUTHOR: "someone" }, { REVIEW_ERROR: "1" }, { BRANCH: "independent" },
    { REVIEW: `Reviewed-by-session: independent ${sha} extra` }, { HAS_PR: "0" }]) {
    const result = run({ FILES: path, ...env });
    assert.notEqual(result.status, 0, JSON.stringify(env) + result.stdout + result.stderr);
    assert.match(result.stdout, new RegExp(`Missing independent session review for HEAD ${sha}`));
    assert.ok(result.stdout.includes(path));
    assert.match(result.stdout, new RegExp(`gh pr comment .*--body 'Reviewed-by-session: <reviewer-session> ${sha}'`));
    assert.doesNotMatch(result.calls, /git \[mac\] push|cargo \[|ssh \[|statuses\/|gh \[mac\] pr merge/);
    if (env.HAS_PR === "0") assert.match(result.stdout, /gh pr create --draft/);
  }
  const reviewed = run({ FILES: path });
  assert.equal(reviewed.status, 0, reviewed.stdout + reviewed.stderr);
  assert.match(reviewed.calls, /gh \[mac\] pr merge/);
  assert.match(reviewed.calls, /api --paginate .*issues\/42\/comments/);
});

test("the review table covers release, CI, native, window, portal and network boundaries", posix, (t) => {
  const run = fixture(t);
  for (const path of ["scripts/sign-update-packages.sh", "scripts/publish-nightly.sh", ".github/workflows/ci.yml",
    "apps/desktop/src-tauri/src/paste/macos.rs", "apps/desktop/src-tauri/capabilities/default.json",
    "apps/desktop/src-tauri/src/picker/portal.rs", "apps/desktop/src-tauri/src/editor_endpoint.rs",
    "scripts/ci-main.mjs", "scripts/ci-result.sh", ".github/workflows/\u00e9 \"quoted\".yml"]) {
    const result = run({ FILES: path, REVIEW: "" });
    assert.notEqual(result.status, 0, path);
    assert.ok(result.stdout.includes(path), result.stdout + result.stderr);
    assert.doesNotMatch(result.calls, /git \[mac\] push|cargo \[|ssh \[|gh \[mac\] pr merge/);
  }
});

test("a review record in a CRLF comment is accepted", posix, (t) => {
  const result = fixture(t)({ FILES: "scripts/land.sh", REVIEW: `Thanks.\r\nReviewed-by-session: independent ${sha}\r\n` });
  assert.equal(result.status, 0, result.stdout + result.stderr);
});

test("docs-only changes need no review; a removed review blocks the final merge", posix, (t) => {
  const run = fixture(t);
  const docs = run({ REVIEW: "", REVIEW_ERROR: "1" });
  assert.equal(docs.status, 0, docs.stdout + docs.stderr);
  assert.doesNotMatch(docs.calls, /issues\/42\/comments/);
  const removed = run({ FILES: "scripts/land.sh", DROP_REVIEW: "1" });
  assert.notEqual(removed.status, 0, removed.stdout + removed.stderr);
  assert.match(removed.calls, /cargo \[mac\] test/);
  assert.doesNotMatch(removed.calls, /statuses\/|gh \[mac\] pr merge/);
});

test("gate property tests override ambient seeds on both machines and report it on failure", posix, (t) => {
  const run = fixture(t);
  for (const machine of ["mac", "ubuntu"]) {
    const result = run({ PROPTEST_RNG_SEED: "987654321", FAIL_TEST_MACHINE: machine });
    assert.notEqual(result.status, 0, result.stdout + result.stderr);
    const seeds = [...result.calls.matchAll(/seed \[(mac|ubuntu)\] (\d+)/g)];
    assert.equal(seeds.length, 2, result.calls);
    assert.equal(seeds[0][2], seeds[1][2]);
    assert.notEqual(seeds[0][2], "987654321");
    assert.ok(result.stdout.includes(`PROPTEST_RNG_SEED=${seeds[0][2]}`), result.stdout);
    assert.doesNotMatch(result.calls, /statuses\/|gh \[mac\] pr merge/);
  }
});

test("invalid titles fail before any machine or push starts", posix, (t) => {
  const run = fixture(t);
  for (const [env, args] of [[{ PR_TITLE: `fix: ${"a".repeat(68)}` }, []], [{ HAS_PR: "0" }, ["--title", `fix: ${"a".repeat(68)}`]]]) {
    const result = run(env, args);
    assert.notEqual(result.status, 0);
    assert.match(result.stdout, /73 characters/);
    assert.doesNotMatch(result.calls, /ssh \[|cargo \[|git \[mac\] push/);
  }
});

test("start claims a branch in a worktree and end removes only a clean merged head", posix, (t) => {
  const run = fixture(t);
  const started = run({}, ["start", "fix-shelf", "7"]);
  assert.equal(started.status, 0, started.stdout + started.stderr);
  assert.match(started.calls, /worktree add -b fix-shelf .*\/ibis-work\/fix-shelf origin\/main/);
  assert.match(started.calls, /pnpm \[mac\] -C apps\/desktop install --frozen-lockfile --offline/);
  assert.match(started.calls, /commit --allow-empty -s -m chore: fix shelf/);
  assert.match(started.calls, /pr create --draft .*--title chore: fix shelf --body Refs #7/);
  for (const env of [{ DIRTY: "1" }, { MERGED: "0" }, { BRANCH: "other" }]) {
    const result = run({ BRANCH: "fix-shelf", ...env }, ["end", "fix-shelf"]);
    assert.notEqual(result.status, 0);
    assert.doesNotMatch(result.calls, /worktree remove|branch -D/);
  }
  const ended = run({ BRANCH: "fix-shelf" }, ["end", "fix-shelf"]);
  assert.equal(ended.status, 0, ended.stdout + ended.stderr);
  assert.match(ended.calls, /worktree remove .*\/ibis-work\/fix-shelf/);
  assert.match(ended.calls, /branch -D fix-shelf/);
  assert.equal(run({}, ["start", "fix-shelf"]).status, 0);
  const alreadyDeleted = run({ BRANCH: "main", BRANCH_EXISTS: "0" }, ["end", "fix-shelf"]);
  assert.equal(alreadyDeleted.status, 0, alreadyDeleted.stdout + alreadyDeleted.stderr);
  assert.doesNotMatch(alreadyDeleted.calls, /branch -D/);
  assert.notEqual(run({}, ["start", "../bad"]).status, 0);
});

test("each machine failure stops before status or merge and reports its first error", posix, (t) => {
  const run = fixture(t);
  for (const machine of ["mac", "ubuntu", "windows"]) {
    const result = run({ FAIL_MACHINE: machine, FILES: "scripts/check-offline.ps1" });
    assert.notEqual(result.status, 0, result.stdout + result.stderr);
    assert.match(result.stdout, /synthetic .*failure/);
    assert.doesNotMatch(result.calls, /statuses\/|gh \[mac\] pr (create|edit|merge)/);
  }
});

test("unreachable machines fail unless explicitly skipped and the skip is recorded", posix, (t) => {
  const run = fixture(t);
  for (const machine of ["ubuntu", "windows"]) {
    const env = { UNREACHABLE: machine, FILES: "scripts/check-offline.ps1", PARALLEL: "0" };
    const failure = run(env);
    assert.notEqual(failure.status, 0, failure.stdout + failure.stderr);
    assert.match(failure.stdout, /unreachable/i);
    assert.doesNotMatch(failure.calls, /statuses\/|gh \[mac\] pr merge/);
    const skipped = run(env, ["--skip", machine]);
    assert.equal(skipped.status, 0, skipped.stdout + skipped.stderr);
    assert.match(skipped.body, new RegExp(`${machine} skipped explicitly`));
    assert.match(skipped.calls, new RegExp(`description=.*${machine} skipped explicitly`));
  }
});

test("existing PR text survives caller overrides; release-app PRs use the same gate", posix, (t) => {
  const run = fixture(t);
  for (const author of ["diyanko", "ibis-release[bot]"]) {
    const result = run({ AUTHOR: author }, ["--title", "ci: caller title", "--body-file", "supplied-body"]);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.match(result.body, /^Reviewed body\.\n\n## Handoff\nExisting instructions\.\n\nLocal gate/);
    assert.match(result.calls, /pr-lint.mjs --title fix: preserve the reviewed title/);
    assert.doesNotMatch(result.calls, /pr edit .*--title/);
    assert.doesNotMatch(result.body, /Caller supplied body/);
  }
  const created = run({ HAS_PR: "0" }, ["--title", "ci: caller title", "--body-file", "supplied-body"]);
  assert.equal(created.status, 0, created.stdout + created.stderr);
  assert.match(created.body, /^Caller supplied body\./);
  assert.match(created.calls, /pr create .*--title ci: caller title/);
});

test("dirty files, moving heads, conflicts, wrong owners and failed publication refuse landing", posix, (t) => {
  const run = fixture(t);
  for (const env of [{ BRANCH: "main" }, { BRANCH: "HEAD" }, { DIRTY: "1" }, { STATUS_ERROR: "74" }, { MOVE_HEAD: "1" }, { CONFLICT: "1" }, { PUSH_STATUS: "73" }, { GH_ACTOR: "someone" }, { AUTHOR: "someone" }, { FORK: "true" }, { LOCK_BUSY: "1", PARALLEL: "0" }, { FETCHED_SHA: "b".repeat(40), PARALLEL: "0" }, { PR_SHA: "b".repeat(40) }]) {
    const result = run(env);
    assert.notEqual(result.status, 0, JSON.stringify(env) + result.stdout + result.stderr);
    assert.doesNotMatch(result.calls, /statuses\/|gh \[mac\] pr merge/);
  }
  assert.doesNotMatch(run({ STATUS_STATUS: "74" }).calls, /gh \[mac\] pr merge/);
  assert.equal(run({ MERGE_STATUS: "74" }).status, 74);
});
