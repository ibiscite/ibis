import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { nightlyConfig } from "./update-feed.mjs";

// The packaging scripts preview.yml runs, on invented build output in a temporary folder.
const skip = process.platform === "win32";
const script = (name) => fileURLToPath(new URL(`./${name}`, import.meta.url));

function build(t, target, packages) {
  const dir = mkdtempSync(join(tmpdir(), "ibis-packages-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  for (const file of ["LICENSE", "NOTICE.md"]) writeFileSync(join(dir, file), file);
  mkdirSync(join(dir, `target/${target}/release/bundle`), { recursive: true });
  for (const path of packages) {
    mkdirSync(join(dir, `target/${target}/release/bundle`, path, ".."), { recursive: true });
    writeFileSync(join(dir, `target/${target}/release/bundle`, path), path);
  }
  mkdirSync(join(dir, "target/cli/release"), { recursive: true });
  writeFileSync(join(dir, "target/cli/release/ibis"), "synthetic command line");
  return (env = {}) => {
    const result = spawnSync("bash", [script("collect-packages.sh")], { cwd: dir, encoding: "utf8", timeout: 10_000,
      env: { ...process.env, TARGET: target, CLI_TARGET: "cli", RUNNER_OS: "Linux", ...env } });
    const dist = existsSync(join(dir, "dist")) ? readdirSync(join(dir, "dist")).sort() : [];
    return { ...result, dist, read: (name) => readFileSync(join(dir, "dist", name), "utf8") };
  };
}

test("packages get plain names per computer and a checksum list that never lists itself", { skip }, (t) => {
  const run = build(t, "x86_64-unknown-linux-gnu", ["deb/ibis_0.3.0_amd64.deb", "rpm/ibis-0.3.0.x86_64.rpm", "appimage/ibis_0.3.0_amd64.AppImage"])();
  assert.equal(run.status, 0, run.stderr);
  assert.deepEqual(run.dist, ["Ibis-Linux.AppImage", "Ibis-Linux.deb", "Ibis-Linux.rpm", "SHA256SUMS-x86_64-unknown-linux-gnu.txt", "ibis-Linux.tar.gz"]);
  const listed = run.read("SHA256SUMS-x86_64-unknown-linux-gnu.txt").trim().split("\n").map((line) => line.split(/\s+\*?/)[1]).sort();
  assert.deepEqual(listed, run.dist.filter((name) => !name.startsWith("SHA256SUMS")));
});

test("no packages, two packages with one name or an unknown computer publish nothing", { skip }, (t) => {
  const empty = build(t, "x86_64-unknown-linux-gnu", [])();
  assert.equal(empty.status, 1);
  assert.match(empty.stderr, /no packages were produced/);
  const twice = build(t, "x86_64-unknown-linux-gnu", ["deb/a.deb", "deb/b.deb"])();
  assert.equal(twice.status, 1);
  assert.match(twice.stderr, /two packages would both be called Ibis-Linux.deb/);
  const unknown = build(t, "riscv64gc-unknown-linux-gnu", ["deb/a.deb"])();
  assert.equal(unknown.status, 1);
  assert.match(unknown.stderr, /no download name for riscv64gc-unknown-linux-gnu/);
  for (const run of [empty, twice, unknown]) assert.ok(!run.dist.some((name) => name.startsWith("SHA256SUMS")), "no checksum list for a failed collection");
});

test("a release tag that differs from the workspace version signs nothing", { skip }, (t) => {
  const root = fileURLToPath(new URL("..", import.meta.url));
  const feed = join(root, "feed");
  assert.ok(!existsSync(feed), "no feed folder before the test");
  t.after(() => rmSync(feed, { recursive: true, force: true }));
  const sign = (args, env) => spawnSync("bash", [script("sign-update-packages.sh"), ...args], { cwd: root, encoding: "utf8", timeout: 10_000,
    env: { ...process.env, REPO: "example/ibis", TAURI_SIGNING_PRIVATE_KEY: "", ...env } });
  const wrong = sign(["release"], { TAG: "v0.0.0-synthetic" });
  assert.equal(wrong.status, 1);
  assert.match(wrong.stdout, /v0\.0\.0-synthetic does not match the workspace version/);
  const unknown = sign(["stable"], {});
  assert.equal(unknown.status, 2);
  assert.ok(!existsSync(feed), "nothing was signed or listed");
});

test("desktop packaging stages the matching CLI before every bundle and keeps standalone archives", () => {
  const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
  const bundle = JSON.parse(read("apps/desktop/src-tauri/tauri.bundle.conf.json")).bundle;
  assert.deepEqual(bundle.externalBin, ["binaries/ibis"]);
  const linux = JSON.parse(read("apps/desktop/src-tauri/tauri.linux.bundle.conf.json")).bundle;
  assert.deepEqual(linux.externalBin, []);
  assert.equal(linux.resources["binaries/ibis"], "binaries/ibis");
  assert.equal(linux.linux, undefined, "Linux CLI resources need no maintainer scripts or dependencies");
  const nightlyDeb = nightlyConfig("0.3.4-nightly.20261112000000", "linux").bundle.linux.deb;
  assert.equal(nightlyDeb.postInstallScript, undefined);
  assert.equal(nightlyDeb.preRemoveScript, undefined);
  const workflow = read(".github/workflows/preview.yml");
  assert.ok(workflow.indexOf("name: Build the command line") < workflow.indexOf("name: Build the packages"));
  assert.match(workflow, /tauri\.bundle\.conf\.json/);
  assert.match(read("scripts/build-cli.sh"), /binaries\/ibis/);
  assert.match(read("scripts/collect-packages.sh"), /CLI_TARGET/);
});

test("Windows packages add only the user PATH and remove their entry on uninstall", () => {
  const read = (name) => readFileSync(new URL(`../apps/desktop/src-tauri/${name}`, import.meta.url), "utf8");
  const windows = JSON.parse(read("tauri.bundle.conf.json")).bundle.windows;
  assert.equal(windows.nsis.installMode, "currentUser");
  assert.match(read(windows.nsis.installerHooks), /NSIS_HOOK_POSTINSTALL/);
  assert.match(read(windows.nsis.installerHooks), /NSIS_HOOK_PREUNINSTALL/);
  const path = read("packaging/path.ps1");
  assert.match(path, /DoNotExpandEnvironmentNames/);
  assert.match(path, /CurrentUser/);
  assert.doesNotMatch(path, /\$env:PATH/);
  assert.match(path, /Remove/);
  assert.doesNotMatch(path, /WriteAllText|Join-Path \$Directory/);
});

test("the CLI build stages the same bytes under the app target, including musl Linux", { skip }, (t) => {
  for (const [runner, target, cliTarget] of [
    ["macOS", "aarch64-apple-darwin", "aarch64-apple-darwin"],
    ["macOS", "x86_64-apple-darwin", "x86_64-apple-darwin"],
    ["Linux", "x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl"],
  ]) {
    const dir = mkdtempSync(join(tmpdir(), "ibis-stage-"));
    t.after(() => rmSync(dir, { recursive: true, force: true }));
    mkdirSync(join(dir, "bin"));
    const binary = "#!/bin/sh\necho ibis-synthetic-matching-build\n";
    writeFileSync(join(dir, "bin/cargo"), `#!/bin/sh\nmkdir -p 'target/${cliTarget}/release'\nprintf '%s' '${binary}' > 'target/${cliTarget}/release/ibis'\nchmod 755 'target/${cliTarget}/release/ibis'\n`, { mode: 0o755 });
    writeFileSync(join(dir, "bin/file"), "#!/bin/sh\necho 'statically linked'\n", { mode: 0o755 });
    const result = spawnSync("bash", [script("build-cli.sh")], { cwd: dir, encoding: "utf8", timeout: 10_000,
      env: { ...process.env, PATH: `${join(dir, "bin")}:${process.env.PATH}`, TARGET: target, RUNNER_OS: runner, GITHUB_ENV: join(dir, "env") } });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(readFileSync(join(dir, `apps/desktop/src-tauri/binaries/ibis-${target}`), "utf8"), binary);
    assert.equal(readFileSync(join(dir, "env"), "utf8"), `CLI_TARGET=${cliTarget}\n`);
    if (runner === "Linux") assert.equal(readFileSync(join(dir, "apps/desktop/src-tauri/binaries/ibis"), "utf8"), binary);
  }
});
