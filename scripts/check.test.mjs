import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { changedFiles } from "./check.mjs";

const checker = fileURLToPath(new URL("./check.mjs", import.meta.url));
const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === "path") ?? "PATH";
const copyChecker = (scripts) => {
  writeFileSync(join(scripts, "check.mjs"), readFileSync(checker));
  writeFileSync(join(scripts, "ci-paths.mjs"), readFileSync(new URL("./ci-paths.mjs", import.meta.url)));
  for (const name of ["empty.test.mjs", "picker-budget.mjs"]) writeFileSync(join(scripts, name), "");
};

test("brand-only checks rebuild without starting Rust or changing source files", { skip: process.platform === "win32" }, (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-brand-check-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, "scripts")); mkdirSync(join(root, "bin"));
  copyChecker(join(root, "scripts"));
  writeFileSync(join(root, "bin/python3"), '#!/bin/sh\nprintf "%s\\n" "$*"\nexit 0\n', { mode: 0o755 });
  const result = spawnSync(process.execPath, [join(root, "scripts/check.mjs"), "--brand-assets"], {
    cwd: root, env: { ...process.env, [pathKey]: `${join(root, "bin")}${delimiter}${process.env[pathKey] ?? ""}` }, encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /brand\/build\.py --check/);
  assert.doesNotMatch(result.stdout, /== Format:|== Desktop:|== Workflow/);
});

test("script-only discovery covers Node and Python names without starting build tools", (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-script-discovery-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const scripts = join(root, "scripts"), bin = join(root, "bin");
  mkdirSync(scripts); mkdirSync(bin);
  copyChecker(scripts);
  for (const name of ["new.test.mjs", "new.test.cjs"]) {
    writeFileSync(join(scripts, name), `console.log(${JSON.stringify(name)});`);
  }
  for (const name of ["test_new.py", "new.test.py", "unrelated.py"]) writeFileSync(join(scripts, name), "");
  const python = process.platform === "win32" ? "python.cmd" : "python3";
  const wrapper = process.platform === "win32"
    ? '@echo off\r\nif "%~1"=="--version" exit /b 0\r\necho PYTHON_TEST %*\r\nexit /b 0\r\n'
    : '#!/bin/sh\nif [ "$1" = "--version" ]; then exit 0; fi\nprintf "PYTHON_TEST %s %s\\n" "$1" "$2"\n';
  writeFileSync(join(bin, python), wrapper, { mode: 0o755 });
  const env = { ...process.env, [pathKey]: `${bin}${delimiter}${process.env[pathKey] ?? ""}` };
  delete env.NODE_TEST_CONTEXT; // The synthetic child runs its own Node test runner.
  const run = (flag) => spawnSync(process.execPath, [join(scripts, "check.mjs"), flag], { cwd: root, env, encoding: "utf8", timeout: 15_000 });
  for (const flag of ["--script-tests", "--platform-script-tests"]) {
    const result = run(flag);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    for (const name of ["new.test.mjs", "new.test.cjs", "test_new.py", "new.test.py"]) assert.match(result.stdout, new RegExp(name));
    assert.doesNotMatch(result.stdout, /unrelated\.py|== Format:|== Lint:|== Desktop:|== Workflow/);
  }
  writeFileSync(join(scripts, "new.test.cjs"), 'throw Error("synthetic script failure");');
  assert.equal(run("--platform-script-tests").status, 1);
  writeFileSync(join(scripts, "new.test.cjs"), "");
  writeFileSync(join(bin, python), process.platform === "win32" ? wrapper.replace(/exit \/b 0\r\n$/, 'exit /b 7\r\n') : wrapper + 'exit 7\n', { mode: 0o755 });
  assert.equal(run("--platform-script-tests").status, 1);
  writeFileSync(join(bin, python), process.platform === "win32" ? '@exit /b 7\r\n' : '#!/bin/sh\nexit 7\n', { mode: 0o755 });
  assert.equal(run("--platform-script-tests").status, 2, "unavailable Python must report incomplete");
});

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), "ibis-check-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const bin = join(root, "tools");
  mkdirSync(bin);
  // Stop at the first normal step; no Rust, package install or app test runs.
  writeFileSync(join(bin, process.platform === "win32" ? "actionlint.cmd" : "actionlint"),
    process.platform === "win32"
      ? '@echo off\r\nif "%~1"=="-version" exit /b 0\r\necho CHECK_STEP_STARTED\r\nexit /b 7\r\n'
      : '#!/bin/sh\nif [ "$1" = "-version" ]; then exit 0; fi\nprintf "CHECK_STEP_STARTED\\n"\nexit 7\n',
    { mode: 0o755 });
  const env = { ...process.env, [pathKey]: `${bin}${delimiter}${process.env[pathKey] ?? ""}` };
  return (args) => spawnSync(process.execPath, [checker, ...args], { cwd: root, env, encoding: "utf8", timeout: 15_000 });
}

test("changed files read committed, uncommitted and untracked work, and anything unclear fails open", () => {
  const answers = (tracked, untracked = "") => (args) => ({ "merge-base": "abc\n", diff: tracked, "ls-files": untracked })[args[0]];
  assert.deepEqual(changedFiles(answers("M\0docs/a.md\0R100\0crates/old.rs\0apps/desktop/new.rs\0D\0gone.txt\0", "scripts/new.mjs\0")), [
    { filename: "scripts/new.mjs", status: "added" },
    { filename: "docs/a.md", previous_filename: null, status: "modified" },
    { filename: "apps/desktop/new.rs", previous_filename: "crates/old.rs", status: "renamed" },
    { filename: "gone.txt", previous_filename: null, status: "removed" },
  ]);
  assert.equal(changedFiles(() => null), null, "no origin/main");
  assert.equal(changedFiles((args) => (args[0] === "diff" ? null : answers("")(args))), null, "failed diff");
  assert.equal(changedFiles((args) => (args[0] === "ls-files" ? null : answers("")(args))), null, "failed listing");
});

test("a change selects the checks CI would run", { skip: process.platform === "win32" }, (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-check-changed-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const scripts = join(root, "scripts"), bin = join(root, "bin"), desktop = join(root, "apps/desktop");
  mkdirSync(scripts); mkdirSync(bin); mkdirSync(desktop, { recursive: true });
  copyChecker(scripts);
  for (const tool of ["actionlint", "cargo", "pnpm", "python3"]) writeFileSync(join(bin, tool), "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  const env = { ...process.env, [pathKey]: `${bin}${delimiter}${process.env[pathKey] ?? ""}`, GIT_AUTHOR_NAME: "t", GIT_AUTHOR_EMAIL: "t@example.org", GIT_COMMITTER_NAME: "t", GIT_COMMITTER_EMAIL: "t@example.org" };
  delete env.NODE_TEST_CONTEXT;
  const git = (...args) => assert.equal(spawnSync("git", args, { cwd: root, env }).status, 0, args.join(" "));
  git("init", "-q", "-b", "main");
  writeFileSync(join(root, ".gitignore"), "scripts\nbin\n");
  git("add", ".gitignore"); git("commit", "-qm", "base");
  git("update-ref", "refs/remotes/origin/main", "HEAD");
  const run = (...args) => spawnSync(process.execPath, [join(scripts, "check.mjs"), ...args], { cwd: root, env, encoding: "utf8", timeout: 60_000 });
  const ran = (result) => [...result.stdout.matchAll(/^== (.+?): (?:actionlint|cargo|node|pnpm|python3) /gm)].map((match) => match[1]);
  const change = (path, text = "x") => { mkdirSync(join(root, path, ".."), { recursive: true }); writeFileSync(join(root, path), text); };
  const steps = (...paths) => {
    for (const path of paths) change(path);
    const result = run();
    assert.equal(result.status, 0, result.stdout + result.stderr);
    return ran(result);
  };
  const docs = steps("docs/note.md");
  assert.ok(docs.includes("Workflow expressions") && docs.includes("Dependency policy"));
  assert.ok(!docs.some((step) => /^(Lint|Test|Format|Desktop: (app|type))/.test(step)), docs.join());
  const front = steps("apps/desktop/src/App.tsx");
  assert.ok(front.includes("Desktop: interface tests") && !front.includes("Desktop: app tests") && !front.includes("Test"), front.join());
  const app = steps("apps/desktop/src-tauri/src/lib.rs");
  assert.ok(app.includes("Desktop: app tests") && app.includes("Format") && !app.includes("Test") && !app.includes("Minimum Rust"), app.join());
  const core = steps("crates/ibis-core/src/lib.rs");
  assert.ok(core.includes("Test") && core.includes("Minimum Rust") && core.includes("Desktop: app tests"), core.join());
  assert.ok(ran(run("--all")).includes("Minimum Rust"));
  git("update-ref", "-d", "refs/remotes/origin/main");
  assert.ok(ran(run()).includes("Test"), "without origin/main everything runs");
});

test("a missing minimum Rust toolchain reports incomplete instead of failed", { skip: process.platform === "win32" }, (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-check-msrv-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const scripts = join(root, "scripts"), bin = join(root, "bin");
  mkdirSync(scripts); mkdirSync(bin); mkdirSync(join(root, "apps/desktop"), { recursive: true });
  copyChecker(scripts);
  for (const tool of ["actionlint", "pnpm", "python3"]) writeFileSync(join(bin, tool), "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  writeFileSync(join(bin, "cargo"), '#!/bin/sh\ncase "$1" in +*) exit 1 ;; esac\nexit 0\n', { mode: 0o755 });
  const env = { ...process.env, [pathKey]: `${bin}${delimiter}${process.env[pathKey] ?? ""}` };
  delete env.NODE_TEST_CONTEXT;
  const result = spawnSync(process.execPath, [join(scripts, "check.mjs"), "--all"], { cwd: root, env, encoding: "utf8", timeout: 60_000 });
  assert.equal(result.status, 2, result.stdout + result.stderr);
  assert.match(result.stderr, /rustup toolchain install 1\.90\.0/);
});

test("unknown arguments are refused before checks", (t) => {
  const run = fixture(t);
  for (const args of [["--unknown"], ["--script-tests", "extra"], ["--expect-commit", "a".repeat(40)]]) {
    const result = run(args);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Usage: node scripts\/check\.mjs/);
    assert.doesNotMatch(result.stdout, /CHECK_STEP_STARTED/);
  }
});

test("ordinary checks start with the first step", (t) => {
  const result = fixture(t)(["--all"]);
  assert.equal(result.status, 1); // The deliberately failed first step.
  assert.match(result.stdout, /== Workflow expressions:/);
  assert.match(result.stdout, /CHECK_STEP_STARTED/);
  assert.doesNotMatch(result.stdout, /== Format:|== Desktop:/);
});
