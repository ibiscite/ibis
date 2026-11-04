import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const runnerMirrors = "http://azure.archive.ubuntu.com/ubuntu/\tpriority:1\nhttps://archive.ubuntu.com/ubuntu/\tpriority:2\nhttps://security.ubuntu.com/ubuntu/\tpriority:3\n";

// scripts/ci-linux-webview.sh with sudo passing through and APT recorded.
function fixture(t) {
  const dir = mkdtempSync(join(tmpdir(), "ibis-ci-apt-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const log = join(dir, "calls"), mirrors = join(dir, "mirrors");
  writeFileSync(join(dir, "sudo"), '#!/usr/bin/env bash\n[[ "$1" == apt-get || "$1" == python3 ]] || exit 98\nexec "$@"\n', { mode: 0o755 });
  writeFileSync(join(dir, "apt-get"), '#!/usr/bin/env bash\nprintf \'%s\\n\' "$*" >> "$APT_LOG"\nif [[ " $* " == *" $FAIL_APT "* ]]; then exit 42; fi\n', { mode: 0o755 });
  const run = (contents, failure = "none") => {
    writeFileSync(log, "");
    writeFileSync(mirrors, contents);
    const result = spawnSync(fileURLToPath(new URL("./ci-linux-webview.sh", import.meta.url)), [mirrors], {
      cwd: dir, encoding: "utf8", timeout: 5000, env: { ...process.env, PATH: `${dir}${delimiter}${process.env.PATH}`, APT_LOG: log, FAIL_APT: failure },
    });
    return { status: result.status, stderr: result.stderr, mirrors: readFileSync(mirrors, "utf8"), calls: readFileSync(log, "utf8") };
  };
  return run;
}

test("Linux WebView downloads prefer the existing HTTPS archive, bound APT and propagate failures", { skip: process.platform === "win32" }, (t) => {
  const run = fixture(t);
  for (const [failure, contents] of [["none", runnerMirrors], ["update", runnerMirrors], ["install", runnerMirrors], ["none", runnerMirrors.replace("priority:2", "priority:0")]]) {
    const result = run(contents, failure);
    assert.equal(result.status, failure === "none" ? 0 : 42, result.stderr);
    assert.equal(result.mirrors, runnerMirrors.replace("priority:2", "priority:0"), "only the already configured archive priority changes");
    const calls = result.calls.trim().split("\n");
    assert.equal(calls.length, failure === "update" ? 1 : 2, "a failed update stops before install");
    for (const call of calls) {
      for (const option of ["Acquire::Retries=1", "Acquire::http::Timeout=15", "Acquire::https::Timeout=15"]) {
        const args = call.split(" ");
        assert.equal(args[args.indexOf(option) - 1], "-o", `${option} is effective for ${call}`);
      }
    }
    assert.match(calls[0], /\bupdate$/);
    if (calls[1]) assert.match(calls[1], /\binstall -y --no-install-recommends libwebkit2gtk-4\.1-dev\b/);
  }
});

test("an unexpected runner mirror list stops before APT without replacing it", { skip: process.platform === "win32" }, (t) => {
  const run = fixture(t);
  for (const contents of ["", runnerMirrors.replace("https://archive.ubuntu.com/ubuntu/", "https://other.example/ubuntu/"), runnerMirrors + "https://archive.ubuntu.com/ubuntu/\tpriority:2\n", runnerMirrors.replace("priority:2", "priority:9")]) {
    const result = run(contents);
    assert.equal(result.status, 1);
    assert.equal(result.calls, "");
    assert.equal(result.mirrors, contents);
  }
});
