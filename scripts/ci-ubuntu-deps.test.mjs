import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const ubuntuSources = `Types: deb
URIs: http://archive.ubuntu.com/ubuntu/
Suites: noble noble-updates noble-backports
Components: main universe
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg

Types: deb
URIs: http://security.ubuntu.com/ubuntu/
Suites: noble-security
Components: main universe
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
`;

function fixture(t) {
  const dir = mkdtempSync(join(tmpdir(), "ibis-ci-apt-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const log = join(dir, "calls"), sources = join(dir, "ubuntu.sources"), used = join(dir, "used.sources");
  for (const [name, script] of Object.entries({
    sudo: 'exec "$@"',
    timeout: 'printf "timeout %s\n" "$1" >> "$APT_LOG"\nshift\nexec "$@"',
    python3: 'exit "$YAML_STATUS"',
    "apt-get": `printf '%s\\n' "$*" >> "$APT_LOG"
for arg; do
  case "$arg" in Dir::Etc::sourcelist=*) cp "\${arg#*=}" "$APT_USED";; esac
done
if [[ " $* " == *" $FAIL_APT "* ]]; then exit 42; fi`,
  })) writeFileSync(join(dir, name), `#!/usr/bin/env bash\n${script}\n`, { mode: 0o755 });

  return (profile, { contents = ubuntuSources, failure = "none", yaml = false } = {}) => {
    writeFileSync(log, "");
    writeFileSync(used, "");
    writeFileSync(sources, contents);
    const result = spawnSync(fileURLToPath(new URL("./ci-ubuntu-deps.sh", import.meta.url)), [profile, sources], {
      cwd: dir, encoding: "utf8", timeout: 5000,
      env: { ...process.env, PATH: `${dir}${delimiter}${process.env.PATH}`, APT_LOG: log, APT_USED: used, FAIL_APT: failure, YAML_STATUS: yaml ? "0" : "1" },
    });
    return { status: result.status, stderr: result.stderr, sources: readFileSync(sources, "utf8"), used: readFileSync(used, "utf8"), calls: readFileSync(log, "utf8").trim().split("\n").filter(Boolean) };
  };
}

const posix = { skip: process.platform === "win32" };

test("desktop CI keeps the runner's mirror choices and stops after a failed index refresh", posix, (t) => {
  const run = fixture(t);
  const ready = run("desktop");
  assert.equal(ready.status, 0, ready.stderr);
  assert.equal(ready.used, "");
  assert.equal(ready.sources, ubuntuSources);
  assert.deepEqual(ready.calls.filter((line) => line.startsWith("timeout ")), ["timeout 5m", "timeout 5m"]);
  assert.match(ready.calls[1], /-o APT::Update::Error-Mode=any update$/);
  assert.match(ready.calls[3], / install -y --no-install-recommends libwebkit2gtk-4\.1-dev\b/);
  const failed = run("desktop", { failure: "update" });
  assert.equal(failed.status, 42, failed.stderr);
  assert.equal(failed.calls.length, 2, "no install after a failed update");
});

test("web CI installs PyYAML from the Azure Ubuntu archive with the same suites and signing key", posix, (t) => {
  const run = fixture(t);
  const ready = run("web");
  assert.equal(ready.status, 0, ready.stderr);
  assert.equal(ready.sources, ubuntuSources);
  assert.equal(ready.used, ubuntuSources.replace("http://archive.ubuntu.com/ubuntu/", "http://azure.archive.ubuntu.com/ubuntu/"));
  assert.match(ready.calls[3], / install -y --no-install-recommends python3-yaml$/);
  assert.deepEqual(run("web", { yaml: true }).calls, []);
  const unknown = run("web", { contents: ubuntuSources.replace("archive.ubuntu.com", "example.invalid") });
  assert.equal(unknown.status, 1, unknown.stderr);
  assert.deepEqual(unknown.calls, []);
});
