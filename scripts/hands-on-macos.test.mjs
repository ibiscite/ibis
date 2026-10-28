// Exercise helpers only: never run the installer, network, app or real settings.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const source = readFileSync(new URL("./hands-on-macos.sh", import.meta.url), "utf8");
const helpers = source.split("# Main:")[0];
const traps = source.match(/^trap cleanup EXIT\ntrap 'exit 130' INT\ntrap 'exit 143' TERM$/m)[0];
const quote = (value) => `'${value.replaceAll("'", "'\\''")}'`;
const skip = process.platform === "win32";
function fixture(t) {
  const root = realpathSync(mkdtempSync(join(tmpdir(), "ibis-hands-on-")));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  return { root, run: (body) => spawnSync("bash", ["-c", `${helpers}\ndir=${quote(root)}\n${body}`], {
    encoding: "utf8", timeout: 10_000,
  }) };
}

test("checksums require exactly one well-formed entry per asset and strict verification", { skip }, (t) => {
  const { root, run } = fixture(t);
  const assets = ["Ibis-macOS-Apple-silicon.dmg", "ibis-macOS-Apple-silicon.tar.gz"];
  const lines = assets.map((name) => {
    writeFileSync(join(root, name), name);
    return `${createHash("sha256").update(name).digest("hex")}  ${name}`;
  });
  const invoke = () => run(`
    system=macOS-Apple-silicon target=aarch64-apple-darwin
    shasum() {
      [ "$*" = '-a 256 --strict -c checked-sums.txt' ] || return 99
      echo STRICT_CHECK >&2
      command shasum "$@"
    }
    verify_downloads`);
  const manifest = join(root, "SHA256SUMS-aarch64-apple-darwin.txt");
  for (const bad of [
    [lines[0]], [lines[1]], [...lines, lines[1]],
    [lines[0], `bad  ${assets[1]}`],
    [lines[0], lines[1].replace("  ", "\t")],
    [...lines, `bad  ${assets[1]}`],
  ]) {
    writeFileSync(manifest, `${bad.join("\n")}\n`);
    const result = invoke();
    assert.notEqual(result.status, 0, result.stdout + result.stderr);
    assert.doesNotMatch(result.stderr, /STRICT_CHECK/, "invalid manifest must fail before verification");
  }
  writeFileSync(manifest, `${lines.join("\n")}\n`);
  const valid = invoke();
  assert.equal(valid.status, 0, valid.stdout + valid.stderr);
  writeFileSync(join(root, assets[1]), "tampered executable");
  const mismatch = invoke();
  assert.notEqual(mismatch.status, 0);
  assert.match(mismatch.stderr, /STRICT_CHECK/);
});

test("Rosetta selects native Apple silicon assets while Intel stays Intel", { skip }, (t) => {
  const { run } = fixture(t);
  for (const [machine, translated, expected] of [
    ["arm64", "", "macOS-Apple-silicon aarch64-apple-darwin"],
    ["x86_64", "1", "macOS-Apple-silicon aarch64-apple-darwin"],
    ["x86_64", "0", "macOS-Intel x86_64-apple-darwin"],
    ["x86_64", "", "macOS-Intel x86_64-apple-darwin"],
  ]) {
    const result = run(`
      uname() { echo ${quote(machine)}; }
      sysctl() { [ "$*" = '-in sysctl.proc_translated' ] || return 99; echo ${quote(translated)}; }
      choose_assets
      echo "$system $target"`);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), expected);
  }
});

test("symlinked files, dangling links and symlinked parents are refused without writing", { skip }, (t) => {
  const { root, run } = fixture(t);
  writeFileSync(join(root, "document"), "keep me");
  mkdirSync(join(root, "outside"));
  symlinkSync(join(root, "document"), join(root, "results.txt"));
  symlinkSync(join(root, "missing"), join(root, "dangling"));
  symlinkSync(join(root, "outside"), join(root, "parent"));
  for (const path of ["results.txt", "dangling", "parent/new-file", "outside"]) {
    assert.notEqual(run(`regular_destination "$dir/${path}"`).status, 0, path);
  }
  assert.equal(run('regular_destination "$dir/new-file"').status, 0);
  for (const path of ["document", "results.txt", "dangling", "parent", "outside"]) {
    assert.notEqual(run(`absent_destination "$dir/${path}"`).status, 0, path);
  }
  assert.equal(run('absent_destination "$dir/new-file"').status, 0);
  assert.equal(readFileSync(join(root, "document"), "utf8"), "keep me");
});

test("exit and signals restore both originals only after stopping; absent originals stay absent", { skip }, (t) => {
  const { root, run } = fixture(t);
  mkdirSync(join(root, "original-settings"));
  writeFileSync(join(root, "original-settings/settings.json"), "saved primary");
  // All mutating commands are functions that only print: no settings are touched.
  const mocks = `
    settings="$dir/unused-settings"; isolated=1
    stop_app() { echo STOPPED; }
    cp() { printf 'COPY %s\\n' "$*"; }
    rm() { printf 'REMOVE %s\\n' "$*"; }
    ${traps}`;
  for (const [action, status] of [["exit 0", 0], ["exit 41", 41], ["kill -INT $$", 130], ["kill -TERM $$", 143]]) {
    const result = run(`${mocks}\n${action}`);
    assert.equal(result.status, status, result.stderr);
    assert.match(result.stdout, /^STOPPED\nCOPY -p .*original-settings\/settings.json .*unused-settings\/settings.json\nREMOVE -f .*unused-settings\/settings.json.bak\n$/);
  }
  writeFileSync(join(root, "original-settings/settings.json.bak"), "saved backup");
  assert.match(run(`${mocks}\nexit 0`).stdout, /COPY -p .*original-settings\/settings.json.bak/);
  const stopped = run(`${mocks}\nstop_app() { return 1; }\nexit 0`);
  assert.notEqual(stopped.status, 0);
  assert.doesNotMatch(stopped.stdout, /COPY|REMOVE/);
  const launched = run(`${mocks}
    isolated=0
    open() { echo OPEN; }
    sleep() { :; }
    launch`);
  assert.equal(launched.status, 0, launched.stderr);
  assert.match(launched.stdout, /^STOPPED\nCOPY .*synthetic-settings.json .*settings.json\nCOPY .*synthetic-settings.json .*settings.json.bak\nOPEN\nSTOPPED\nCOPY -p /);
});

test("Zotero consent EOF exits instead of looping or allowing access", { skip }, (t) => {
  const { run } = fixture(t);
  const result = run('exec 3</dev/null\nif opt_in_zotero; then echo ALLOWED; fi');
  assert.equal(result.status, 1, result.stderr);
  assert.doesNotMatch(result.stdout, /ALLOWED/);
});

test("the multi-reference prompt supplies a query with multiple synthetic matches", () => {
  const query = source.match(/ask "Search ([^,]+), press Shift\+Enter on two references/)[1].toLowerCase().split(/\s+/);
  const records = readFileSync(new URL("../fixtures/bibtex/real-world.bib", import.meta.url), "utf8").toLowerCase().split(/^@/m);
  assert.ok(records.filter((record) => query.every((term) => record.includes(term))).length >= 2);
});

test("failed install cleanup always detaches, rolls back and retains an unrecoverable previous app", { skip }, (t) => {
  const { root, run } = fixture(t);
  mkdirSync(join(root, "stage/previous.app"), { recursive: true });
  const mocks = `
    app="$dir/unused-app"; stage="$dir/stage"; mount="$dir/mount"; swapping=1; had_app=1
    hdiutil() { echo "IMAGE $*"; }
    rm() { echo "REMOVE $*"; }
    mv() { echo "MOVE $*"; }
    ${traps}`;
  const result = run(`${mocks}\nexit 41`);
  assert.equal(result.status, 41, result.stderr);
  assert.match(result.stdout, /IMAGE detach -quiet/);
  assert.match(result.stdout, /REMOVE -rf .*unused-app\nMOVE .*previous.app .*unused-app\nREMOVE -rf .*stage\n/);
  const failed = run(`${mocks}\nmv() { return 1; }\nexit 41`);
  assert.notEqual(failed.status, 0);
  assert.match(failed.stderr, /rollback failed/);
  assert.doesNotMatch(failed.stdout, /REMOVE -rf .*stage/);
  const copyFailure = run(`${mocks}\nswapping=0\nexit 41`);
  assert.equal(copyFailure.status, 41);
  assert.match(copyFailure.stdout, /IMAGE detach/);
  assert.doesNotMatch(copyFailure.stdout, /unused-app/);
});
