import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
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
  const log = join(dir, "calls"), sources = join(dir, "ubuntu.sources"), used = join(dir, "used.sources"), config = join(dir, "used.conf");
  for (const [name, script] of Object.entries({
    sudo: 'exec "$@"',
    timeout: 'printf "timeout %s\n" "$1" >> "$APT_LOG"\nshift\nexec "$@"',
    python3: 'exit "$YAML_STATUS"',
    sha256sum: `exec "$NODE_FOR_TEST" -e 'const { createHash } = require("node:crypto"); const { readFileSync } = require("node:fs"); console.log(createHash("sha256").update(readFileSync(0)).digest("hex") + "  -");'`,
    "apt-get": `printf '%s\\n' "$*" >> "$APT_LOG"
previous=''
for arg; do
  if [ "$previous" = -c ]; then cp "$arg" "$APT_CONFIG_USED"; fi
  case "$arg" in Dir::Etc::sourcelist=*) cp "\${arg#*=}" "$APT_USED";; esac
  previous=$arg
done
if [[ " $* " == *" $FAIL_APT "* ]]; then exit 42; fi`,
  })) writeFileSync(join(dir, name), `#!/usr/bin/env bash\n${script}\n`, { mode: 0o755 });

  return (profile, { contents = ubuntuSources, failure = "none", yaml = false } = {}) => {
    writeFileSync(log, "");
    writeFileSync(used, "");
    writeFileSync(config, "");
    writeFileSync(sources, contents);
    const result = spawnSync(fileURLToPath(new URL("./ci-ubuntu-deps.sh", import.meta.url)), Array.isArray(profile) ? profile : [profile, sources], {
      cwd: dir, encoding: "utf8", timeout: 5000,
      env: { ...process.env, PATH: `${dir}${delimiter}${process.env.PATH}`, APT_LOG: log, APT_USED: used, APT_CONFIG_USED: config, NODE_FOR_TEST: process.execPath, FAIL_APT: failure, YAML_STATUS: yaml ? "0" : "1" },
    });
    return { status: result.status, stdout: result.stdout, stderr: result.stderr, sources: readFileSync(sources, "utf8"), used: readFileSync(used, "utf8"), config: readFileSync(config, "utf8"), calls: readFileSync(log, "utf8").trim().split("\n").filter(Boolean) };
  };
}

const posix = { skip: process.platform === "win32" };

// What CI reads: the archive stanza alone, the security pocket left out.
const archiveOnly = ubuntuSources.slice(0, ubuntuSources.indexOf("\nTypes: deb\nURIs: http://security"));

test("desktop CI keeps the runner's mirror choices without the security pocket and stops after a failed index refresh", posix, (t) => {
  const run = fixture(t);
  const ready = run("desktop");
  assert.equal(ready.status, 0, ready.stderr);
  assert.equal(ready.used, archiveOnly);
  assert.equal(ready.sources, ubuntuSources, "the runner's own file is untouched");
  assert.deepEqual(ready.calls.filter((line) => line.startsWith("timeout ")), ["timeout 15m", "timeout 15m"]);
  assert.match(ready.calls[1], /-o Dir::Etc::sourceparts=- -o APT::Update::Error-Mode=any update$/);
  assert.match(ready.calls[3], / install -y --no-install-recommends libwebkit2gtk-4\.1-dev\b/);
  const failed = run("desktop", { failure: "update" });
  assert.equal(failed.status, 42, failed.stderr);
  assert.equal(failed.calls.length, 2, "no install after a failed update");
});

test("only security suites leave the sources, and a file with nothing else stops before apt", posix, (t) => {
  const run = fixture(t);
  const mixed = run("desktop", { contents: ubuntuSources.replace("Suites: noble noble-updates noble-backports", "Suites: noble noble-security noble-updates") });
  assert.equal(mixed.status, 0, mixed.stderr);
  assert.match(mixed.used, /^Suites: noble noble-updates$/m);
  assert.doesNotMatch(mixed.used, /security/);
  const onlySecurity = run("desktop", { contents: ubuntuSources.slice(ubuntuSources.indexOf("Types: deb\nURIs: http://security")) });
  assert.equal(onlySecurity.status, 1, onlySecurity.stderr);
  assert.deepEqual(onlySecurity.calls, []);
});

test("web CI installs PyYAML from the Azure Ubuntu archive with the same suites and signing key", posix, (t) => {
  const run = fixture(t);
  const ready = run("web");
  assert.equal(ready.status, 0, ready.stderr);
  assert.equal(ready.sources, ubuntuSources);
  assert.equal(ready.used, archiveOnly.replace("http://archive.ubuntu.com/ubuntu/", "http://azure.archive.ubuntu.com/ubuntu/"));
  assert.match(ready.calls[3], / install -y --no-install-recommends python3-yaml$/);
  assert.deepEqual(run("web", { yaml: true }).calls, []);
  const unknown = run("web", { contents: ubuntuSources.replace("archive.ubuntu.com", "example.invalid") });
  assert.equal(unknown.status, 1, unknown.stderr);
  assert.deepEqual(unknown.calls, []);
});

test("cache keys hash exactly the installed package list without touching apt or needing sources", posix, (t) => {
  const run = fixture(t);
  for (const profile of ["desktop", "web"]) {
    const installed = run(profile);
    const packages = installed.calls.at(-1).split("--no-install-recommends ")[1].split(" ");
    const key = run(["cache-key", profile]);
    assert.equal(key.status, 0, key.stderr);
    assert.equal(key.stdout, `key=${createHash("sha256").update(packages.join("\n") + "\n").digest("hex")}\n`);
    assert.equal(run(["cache-key", profile], { yaml: true }).stdout, key.stdout);
    assert.deepEqual(key.calls, []);
  }
});

test("apt retains downloads for the cache even in images with cleanup hooks", posix, (t) => {
  const run = fixture(t);
  for (const profile of ["desktop", "web"]) {
    const result = run(profile);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.config, /^#clear DPkg::Post-Invoke;$/m);
    assert.match(result.config, /^#clear APT::Update::Post-Invoke;$/m);
    assert.match(result.config, /^APT::Keep-Downloaded-Packages "true";$/m);
    assert.match(result.config, /^Binary::apt::APT::Keep-Downloaded-Packages "true";$/m);
  }
});
