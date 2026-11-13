import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const sha = "a".repeat(40), posix = { skip: process.platform === "win32" };
function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), "ibis-land-")), bin = join(root, "bin"), calls = join(root, "calls");
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const path of ["bin", "scripts", "ubuntu/Code/ibis/.git", "ubuntu/.cargo", "ubuntu/.nvm"]) mkdirSync(join(root, path), { recursive: true });
  writeFileSync(join(root, "ubuntu/.cargo/env"), "");
  writeFileSync(join(root, "ubuntu/.nvm/nvm.sh"), "nvm() { return 0; }\n");
  writeFileSync(join(root, "scripts/check-synthetic.mjs"), "");
  writeFileSync(join(root, "scripts/synthetic.test.mjs"), "");
  writeFileSync(join(root, "supplied-body"), "Caller supplied body.\n");
  for (const [name, body] of Object.entries({
    git: `case "$1" in
      rev-parse) case "$2" in --abbrev-ref) echo "$BRANCH";; --show-toplevel) pwd;; FETCH_HEAD) echo "$FETCHED_SHA";; *) if [ "$MACHINE" = mac ] && [ -f "$ROOT/changed" ]; then echo bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb; else echo "$SHA"; fi;; esac;;
      status) [ "$STATUS_ERROR" = 0 ] || exit "$STATUS_ERROR"; if [ "$DIRTY" = 1 ]; then echo ' M synthetic'; fi;;
      merge-tree) exit "$CONFLICT";;
      rev-list) echo "$SHA";;
      show) if [ "$3" = --format=%s ]; then echo 'ci: simplify the pipeline'; else printf 'Synthetic change.\\n\\nCloses #7\\n\\nSigned-off-by: Synthetic <synthetic@example.org>\\nAssisted-by: Codex\\n'; fi;;
      diff) printf '%s\\n' "$FILES";;
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
    fi`,
    pnpm: `exit 0`,
    node: `case "$1" in
      --input-type=module) exec "$REAL_NODE" "$@";;
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
        if [ "$DROP_LAUNCH" = 1 ] && [[ "$*" == *'bash -s --'* ]]; then bash -c "$*"; exit 255; fi
        exec bash -c "$*"
      else
        cat > "$ROOT/windows-input"
        [ "$UNREACHABLE" != windows ] || { echo 'ssh: Windows unreachable' >&2; exit 255; }
        [ "$FAIL_MACHINE" != windows ] || { echo 'Error: synthetic Windows failure'; exit 1; }
      fi`,
    gh: `case "$1 $2" in
      'api user') echo "$GH_ACTOR";;
      'repo view') echo example/ibis;;
      'pr list') if [ "$HAS_PR" = 1 ]; then printf '42\\t%s\\t%s\\n' "$AUTHOR" "$FORK"; fi;;
      'pr view') case "$*" in *--json\\ title*) echo 'fix: preserve the reviewed title';; *--json\\ body*) printf 'Reviewed body.\\n\\n## Handoff\\nExisting instructions.\\n';; *--json\\ headRefOid*) echo "$PR_SHA";; *) echo https://example.invalid/pull/42;; esac;;
      'pr create'|'pr edit') while [ "$#" -gt 0 ]; do if [ "$1" = --body-file ]; then cp "$2" "$ROOT/body"; fi; shift; done; echo https://example.invalid/pull/42;;
      'api repos/'*) exit "$STATUS_STATUS";;
      'pr merge') exit "$MERGE_STATUS";;
    esac`,
  })) writeFileSync(join(bin, name), `#!/usr/bin/env bash\nprintf '${name} [%s] %s\\n' "$MACHINE" "$*" >> "$CALLS"\n${body}\n`, { mode: 0o755 });
  return (overrides = {}, args = []) => {
    for (const path of ["changed", "body", "mac-started", "ubuntu-started", "launched", "dropped", "dropped-log"]) rmSync(join(root, path), { force: true });
    writeFileSync(calls, "");
    const result = spawnSync("bash", [fileURLToPath(new URL("./land.sh", import.meta.url)), ...args], { cwd: root, encoding: "utf8", timeout: 15000,
      env: { ...process.env, PATH: `${bin}${delimiter}${process.env.PATH}`, ROOT: root, CALLS: calls, REAL_NODE: process.execPath, MACHINE: "mac", BRANCH: "ci/synthetic", SHA: sha, PR_SHA: sha, FETCHED_SHA: sha, DIRTY: "0", MOVE_HEAD: "0", STATUS_ERROR: "0", GH_ACTOR: "diyanko", CONFLICT: "0", PUSH_STATUS: "0", HAS_PR: "1", AUTHOR: "diyanko", FORK: "false", MERGE_STATUS: "0", STATUS_STATUS: "0", FAIL_MACHINE: "", UNREACHABLE: "", LOCK_BUSY: "0", DROP_POLL: "0", DROP_LAUNCH: "0", DROP_LOG: "0", FILES: "docs/testing.md", PARALLEL: "1", ...overrides } });
    let body = ""; try { body = readFileSync(join(root, "body"), "utf8"); } catch {}
    return { ...result, calls: readFileSync(calls, "utf8"), body };
  };
}

test("parallel gates set Local gate on the checked SHA and immediately squash merge", posix, (t) => {
  const result = fixture(t)({ HAS_PR: "0", FILES: "scripts/land.sh", DROP_POLL: "1", DROP_LAUNCH: "1", DROP_LOG: "1" });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.calls, /cargo \[mac\] clippy --workspace --all-targets --locked/);
  assert.match(result.calls, /cargo \[ubuntu\] test --workspace --locked/);
  assert.match(result.calls, /pnpm \[mac\].*tsc --noEmit/);
  assert.match(result.calls, /node \[mac\] --test --test-concurrency=2 scripts\/synthetic.test.mjs/);
  assert.match(result.calls, /node \[mac\] scripts\/gen.mjs --headless --check/);
  assert.match(result.calls, new RegExp(`gh \\[mac\\] api repos/example/ibis/statuses/${sha} .*context=Local gate.*state=success`));
  assert.match(result.calls, new RegExp(`gh \\[mac\\] pr merge .*--squash --match-head-commit ${sha}`));
  assert.doesNotMatch(result.calls, /--auto|--admin|target_url/);
  assert.ok(result.calls.indexOf("git [mac] push") < result.calls.indexOf("cargo [mac] clippy"));
  assert.ok(result.calls.indexOf("cargo [ubuntu] test") < result.calls.indexOf("gh [mac] pr create"));
  assert.match(result.body, /Synthetic change\.[\s\S]*Closes #7[\s\S]*Local gate.*mac passed; ubuntu passed/);
  assert.doesNotMatch(result.body, /Signed-off-by:|Assisted-by:/);
  const windows = Buffer.from(result.calls.match(/-EncodedCommand ([A-Za-z0-9+/=]+)/)[1], "base64").toString("utf16le");
  assert.match(windows, /PSVersionTable\.PSVersion\.Major -ne 5/);
  assert.match(windows, /powershell\.exe .* -File \$test.FullName/);
  assert.match(result.body, /windows passed/);
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
