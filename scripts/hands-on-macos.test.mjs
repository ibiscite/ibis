// Exercise helpers only: never run the installer, network, app or real settings.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, readlinkSync, realpathSync, rmSync, statSync, symlinkSync, writeFileSync } from "node:fs";
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

test("a live bash/sh PID holds the lock, including curl | bash without a script argument", { skip }, (t) => {
  const { root, run } = fixture(t);
  const lock = join(root, "profile.hands-on.lock");
  const owner = join(lock, `pid-${process.pid}-live`);
  for (const [command, args, holds] of [
    ["bash", "bash", true], // curl ... | bash has no script-name argument.
    ["/bin/bash", "bash /tmp/hands-on-macos.sh", true],
    ["sh", "sh scripts/hands-on-macos.sh", true],
    ["node", "node scripts/hands-on-macos.sh", false],
    ["/bin/bash", "bash other.sh", true],
    ["/bin/bash", "bash hands-on-macos.sh.old", true],
    ["", "", false],
  ]) {
    mkdirSync(lock);
    writeFileSync(owner, `${process.pid}\n`);
    const result = run(`settings="$dir/profile"
      ps() {
        case "$*" in
          '-o comm= -p ${process.pid}') printf '%s\\n' ${quote(command)} ;;
          '-o args= -p ${process.pid}') printf '%s\\n' ${quote(args)} ;;
          *) return 99 ;;
        esac
      }
      ${traps}
      acquire_lock`);
    if (holds) {
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, /Another hands-on check holds this profile lock/);
      assert.ok(result.stderr.includes(`Lock directory: ${lock}.`));
      assert.match(result.stderr, /It can be deleted if no other hands-on check is running/);
      assert.equal(readFileSync(owner, "utf8"), `${process.pid}\n`);
      rmSync(lock, { recursive: true });
    } else {
      assert.equal(result.status, 0, result.stderr);
      assert.equal(existsSync(lock), false);
    }
  }
});

test("a dead PID lock is cleared, claimed and released on exit or signals", { skip }, (t) => {
  const { root, run } = fixture(t);
  const dead = run('printf "%s" "$$"').stdout;
  const lock = join(root, "profile.hands-on.lock");
  for (const [action, status] of [["exit 0", 0], ["exit 41", 41], ["kill -INT $$", 130], ["kill -TERM $$", 143]]) {
    mkdirSync(lock);
    writeFileSync(join(lock, `pid-${dead}-stale`), ""); // Killed before writing contents.
    const result = run(`settings="$dir/profile"
      ${traps}
      acquire_lock
      IFS= read -r owner < "$lock_record"
      [ "$owner" = "$$" ]
      ${action}`);
    assert.equal(result.status, status, result.stderr);
    assert.equal(existsSync(lock), false);
  }
});

test("stale-lock recovery and failed acquisition preserve a replacement owner", { skip }, (t) => {
  const { root, run } = fixture(t);
  const dead = run('printf "%s" "$$"').stdout;
  const lock = join(root, "profile.hands-on.lock");
  mkdirSync(lock);
  writeFileSync(join(lock, `pid-${dead}-stale`), `${dead}\n`);
  const replacement = join(lock, `pid-${dead}-replacement`);
  const result = run(`settings="$dir/profile"
    kill() {
      command rm -f "$lock/pid-${dead}-stale"
      printf '%s\\n' ${quote(dead)} > "$lock/pid-${dead}-replacement"
      return 1
    }
    ${traps}
    acquire_lock`);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Another hands-on check acquired the profile lock/);
  assert.equal(readFileSync(replacement, "utf8"), `${dead}\n`);
  assert.equal(existsSync(join(lock, `pid-${dead}-stale`)), false);
});

test("an empty abandoned lock is reusable and unknown entries are refused", { skip }, (t) => {
  const { root, run } = fixture(t);
  const lock = join(root, "profile.hands-on.lock");
  mkdirSync(lock);
  const empty = run(`settings="$dir/profile"\n${traps}\nacquire_lock`);
  assert.equal(empty.status, 0, empty.stderr);
  assert.equal(existsSync(lock), false);
  mkdirSync(lock);
  writeFileSync(join(lock, ".keep"), "leave me untouched");
  const unknown = run(`settings="$dir/profile"\n${traps}\nacquire_lock`);
  assert.notEqual(unknown.status, 0);
  assert.match(unknown.stderr, /Unrecognized profile lock entry/);
  assert.equal(readFileSync(join(lock, ".keep"), "utf8"), "leave me untouched");
});

function profileFixture(t, present = true) {
  const f = fixture(t);
  writeFileSync(join(f.root, "synthetic-settings.json"), "synthetic preferences");
  writeFileSync(join(f.root, "untouched"), "outside the profile");
  if (present) {
    mkdirSync(join(f.root, "profile/nested"), { recursive: true });
    for (const [name, text] of Object.entries({ "settings.json": "saved primary", "settings.json.bak": "saved backup", "apps.json": "saved per-app choices", "future.json": "another setting", "nested/keep": "nested data" })) writeFileSync(join(f.root, "profile", name), text);
    chmodSync(join(f.root, "profile"), 0o750);
    chmodSync(join(f.root, "profile/apps.json"), 0o600);
    symlinkSync("../untouched", join(f.root, "profile/link"));
  }
  return f;
}

function assertOriginal(root, profile = "profile") {
  assert.deepEqual(readdirSync(join(root, profile)).sort(), ["apps.json", "future.json", "link", "nested", "settings.json", "settings.json.bak"]);
  for (const [name, text] of Object.entries({ "settings.json": "saved primary", "settings.json.bak": "saved backup", "apps.json": "saved per-app choices", "future.json": "another setting", "nested/keep": "nested data" })) assert.equal(readFileSync(join(root, profile, name), "utf8"), text);
  assert.equal(readlinkSync(join(root, profile, "link")), "../untouched");
  assert.equal(statSync(join(root, profile)).mode & 0o777, 0o750);
  assert.equal(statSync(join(root, profile, "apps.json")).mode & 0o777, 0o600);
  assert.equal(readFileSync(join(root, "untouched"), "utf8"), "outside the profile");
}

const profileSetup = `settings="$dir/profile"
  stop_app() { :; }
  ${traps}`;

test("a whole synthetic profile is isolated and every original returns on exit or interruption", { skip }, (t) => {
  for (const [action, status] of [["exit 0", 0], ["exit 41", 41], ["kill -INT $$", 130], ["kill -TERM $$", 143]]) {
    const { root, run } = profileFixture(t);
    const result = run(`${profileSetup}
      seed_settings
      [ ! -e "$settings/apps.json" ] && [ ! -e "$settings/future.json" ] && [ ! -e "$settings/link" ]
      printf 'test app choice' > "$settings/apps.json"
      printf 'new test file' > "$settings/new-file.json"
      printf 'recovery artifact' > "$settings/settings.json.invalid-1"
      seed_settings
      [ "$(cat "$settings/apps.json")" = 'test app choice' ]
      ${action}`);
    assert.equal(result.status, status, result.stderr);
    assertOriginal(root);
    assert.equal(readdirSync(root).some(name => name.startsWith("profile.hands-on-original.")), false);
  }
});

test("a profile absent before testing stays absent after completion and interruption", { skip }, (t) => {
  for (const [action, status] of [["exit 0", 0], ["kill -TERM $$", 143]]) {
    const { root, run } = profileFixture(t, false);
    const result = run(`${profileSetup}\nseed_settings\nprintf 'test choice' > "$settings/apps.json"\n${action}`);
    assert.equal(result.status, status, result.stderr);
    assert.equal(existsSync(join(root, "profile")), false);
    assert.equal(readdirSync(root).some(name => name.startsWith("profile.hands-on-original.")), false);
  }
});

test("failed initial movement or seeding leaves the original profile intact", { skip }, (t) => {
  for (const failure of [
    `mv() { [ "$1" != "$settings" ] || return 74; command mv "$@"; }`,
    `cp() { return 73; }`,
    `stop_app() { return 76; }`,
  ]) {
    for (const invoke of ["seed_settings", "if seed_settings; then exit 0; else exit 77; fi"]) {
      const { root, run } = profileFixture(t);
      const result = run(`${profileSetup}\n${failure}\n${invoke}`);
      assert.notEqual(result.status, 0);
      assertOriginal(root);
    }
  }
});

test("failed restoration preserves the whole original and refuses a later test session", { skip }, (t) => {
  for (const failure of [
    `mv() { [ "$1" != "$profile_stage/original" ] || return 75; command mv "$@"; }`,
    `stop_app() { [ "$isolated" = 0 ]; }`,
  ]) {
    const { root, run } = profileFixture(t);
    const result = run(`${profileSetup}\n${failure}\nseed_settings\nprintf 'test choice' > "$settings/apps.json"`);
    assert.notEqual(result.status, 0);
    const backups = readdirSync(root).filter(name => name.startsWith("profile.hands-on-original."));
    assert.equal(backups.length, 1);
    assertOriginal(root, `${backups[0]}/original`);
    // A failed final rename can leave the destination absent; the original
    // remains complete. A new session must leave either state untouched.
    const testProfilePresent = existsSync(join(root, "profile"));
    if (testProfilePresent) assert.equal(readFileSync(join(root, "profile/apps.json"), "utf8"), "test choice");
    const again = run(`${profileSetup}\nseed_settings`);
    assert.notEqual(again.status, 0);
    assert.match(again.stderr, /earlier hands-on check/i);
    assertOriginal(root, `${backups[0]}/original`);
    assert.equal(existsSync(join(root, "profile")), testProfilePresent);
    if (testProfilePresent) assert.equal(readFileSync(join(root, "profile/apps.json"), "utf8"), "test choice");
  }
});

test("a symlinked profile is refused before isolation touches its target", { skip }, (t) => {
  const { root, run } = profileFixture(t, false);
  mkdirSync(join(root, "outside"));
  writeFileSync(join(root, "outside/apps.json"), "keep this choice");
  symlinkSync(join(root, "outside"), join(root, "profile"));
  const result = run(`${profileSetup}\nseed_settings`);
  assert.notEqual(result.status, 0);
  assert.equal(readFileSync(join(root, "outside/apps.json"), "utf8"), "keep this choice");
});

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
