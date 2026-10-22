import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { classify } from "./ci-paths.mjs";

const full = { native: true, web: true, nix: true };
const row = (filename, previous_filename = null) => ({ filename, previous_filename, status: previous_filename ? "renamed" : "modified" });
const pr = (files, expected = files.length) => classify("pull_request", expected, files);

function workflowStep() {
  const workflow = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
  const step = workflow.match(/id: classify\n([\s\S]*?)\n  test:/)[1];
  const shell = step.match(/^        shell: (\S+)$/m)?.[1];
  assert.ok(shell === undefined || shell === "bash");
  // GitHub adds pipefail only for an explicitly selected bash shell.
  return { command: step.match(/        run: \|\n([\s\S]*)/)[1].replace(/^          /gm, ""), args: shell === "bash" ? ["-e", "-o", "pipefail"] : ["-e"] };
}

test("interface-only changes retain web checks without native jobs", () => {
  for (const path of ["apps/desktop/src/App.tsx", "apps/desktop/src/features/picker/PickerFooter.css", "apps/desktop/src/messages.ts", "apps/desktop/src/icon.svg", "apps/desktop/index.html", "apps/desktop/gallery.html", "scripts/gallery-contact-sheet.mjs", "scripts/gallery-geometry.mjs", "scripts/gallery-geometry.test.mjs"]) {
    assert.deepEqual(pr([row(path), row("docs/design.md")]), { native: false, web: true, nix: false }, path);
  }
});

test("documentation keeps its existing fast path, except synthetic fixtures", () => {
  assert.deepEqual(pr([row("README.md"), row("docs/design/menu.md"), row("LICENSE"), row(".github/ISSUE_TEMPLATE/task.yml")]), { native: false, web: false, nix: false });
  assert.equal(pr([row("fixtures/README.md")]).native, true);
});

test("native, tooling and unknown changes stay conservative", () => {
  for (const path of ["crates/ibis-desktop/src/lib.rs", "Cargo.toml", "apps/desktop/vite.config.ts", "apps/desktop/src/unknown.rs", "apps/desktop/src-tauri/tauri.conf.json", "scripts/smoke-native.py", "scripts/update-e2e.py", ".github/workflows/ci.yml", "unknown/file", "docs/design-geometry.json"]) {
    assert.deepEqual(pr([row("apps/desktop/src/App.tsx"), row(path)]), { native: true, web: true, nix: false }, path);
  }
});

test("Nix inputs retain their dedicated build", () => {
  for (const path of ["flake.nix", "flake.lock", "nix/pnpm-deps-hash", "Cargo.lock", "apps/desktop/package.json", "apps/desktop/pnpm-lock.yaml"]) {
    assert.deepEqual(pr([row(path)]), full, path);
  }
});

test("both sides of a rename determine the required checks", () => {
  assert.deepEqual(pr([row("apps/desktop/src/Renamed.tsx", "apps/desktop/src/App.tsx")]), { native: false, web: true, nix: false });
  assert.equal(pr([row("docs/old.md", "crates/ibis-desktop/src/lib.rs")]).native, true);
  assert.equal(pr([row("docs/old.md", "Cargo.lock")]).nix, true);
  assert.deepEqual(pr([{ filename: "README.md", status: "renamed" }]), full);
});

test("empty, incomplete, malformed or ambiguous file lists run everything", () => {
  for (const files of [[], null, {}, [null], [row(12)], [row("")], [row("/README.md")], [row("../README.md")], [row("apps/desktop/src/../native.md")], [row("README.md\n")], [row("docs/\u0000.md")], [row("apps\\desktop\\src\\App.tsx")], [row("README.md", 42)], [row("README.md"), row("README.md")]]) {
    assert.deepEqual(pr(files, 1), full);
  }
  for (const expected of [0, -1, 2, "1", NaN, 3001]) assert.deepEqual(pr([row("README.md")], expected), full);
  assert.deepEqual(pr(Array.from({ length: 3001 }, (_, i) => row(`docs/${i}.md`))), full);
});

test("Unicode paths work without interpreting markup or shell syntax", () => {
  assert.deepEqual(pr([row("apps/desktop/src/研究.tsx"), row("apps/desktop/src/مرحبا.css"), row("docs/$(false)`x`.md")]), { native: false, web: true, nix: false });
});

test("main and merge queue always validate the complete app", () => {
  for (const event of ["push", "merge_group", "unknown"]) assert.deepEqual(classify(event, 1, [row("README.md")]), { native: true, web: true, nix: false });
  assert.deepEqual(classify("workflow_dispatch"), full);
  assert.deepEqual(classify("schedule"), { native: false, web: false, nix: true });
});

test("the CLI emits only fixed decisions for valid, missing and malformed metadata", (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-ci-paths-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const file = join(dir, "synthetic.ndjson");
  for (const [contents, expected] of [[JSON.stringify(row("apps/desktop/src/研究.tsx")), "native=false\nweb=true\nnix=false\n"], ["private-資料: invalid metadata", "native=true\nweb=true\nnix=true\n"], ["", "native=true\nweb=true\nnix=true\n"], [null, "native=true\nweb=true\nnix=true\n"]]) {
    if (contents !== null) writeFileSync(file, contents);
    else rmSync(file);
    const result = spawnSync(process.execPath, [fileURLToPath(new URL("./ci-paths.mjs", import.meta.url))], { encoding: "utf8", env: { ...process.env, EVENT: "pull_request", EXPECTED: "1", FILES: file }, timeout: 5000 });
    assert.equal(result.status, 0);
    assert.equal(result.stdout, expected);
    assert.equal(result.stderr, "");
  }
  assert.deepEqual(pr([{ filename: "README.md" }]), full);
});

test("workflow retains web validation and gates native-only Windows bootstrap", () => {
  const workflow = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
  assert.match(workflow, /- name: Type check and test the interface\n        if: env.WEB == 'true'/);
  assert.match(workflow, /- name: Check Windows updater activation contracts\n        if: env.NATIVE == 'true' && runner.os == 'Windows'/);
  assert.match(workflow, /EXPECTED_REFS:.*pull_request.head.sha.*pull_request.base.sha/);
  assert.equal(workflow.match(/= "\$EXPECTED_REFS"/g)?.length, 2);
});

test("the real workflow shell falls back on failed reads or changing refs", { skip: process.platform === "win32" }, (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-ci-refs-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const { command, args } = workflowStep();
  const fakeApi = `gh() {
    if [[ "$*" == *"/files"* ]]; then
      printf '%s\\n' "$METADATA"
      [[ "$SCENARIO" != failedfiles ]]
    else
      if [[ "$SCENARIO" == changedrefs && -f "$RUNNER_TEMP/read" ]]; then printf 'changed\\n'; else printf '%s\\n' "$EXPECTED_REFS"; fi
      touch "$RUNNER_TEMP/read"
      [[ "$SCENARIO" != failedhead ]]
    fi
  }`;
  for (const scenario of ["valid", "changedrefs", "failedhead", "failedfiles", "partial", "malformed"]) {
    rmSync(join(dir, "read"), { force: true });
    const result = spawnSync("bash", [...args, "-c", `${fakeApi}\n${command}`], {
      cwd: fileURLToPath(new URL("..", import.meta.url)), encoding: "utf8", timeout: 5000,
      env: { ...process.env, EVENT: "pull_request", EXPECTED: scenario === "partial" ? "2" : "1", EXPECTED_REFS: `${"a".repeat(40)}:${"b".repeat(40)}`, RUNNER_TEMP: dir, GITHUB_OUTPUT: join(dir, "output"), SCENARIO: scenario, METADATA: scenario === "malformed" ? "invalid-private-資料" : JSON.stringify(row("apps/desktop/src/App.tsx")) },
    });
    assert.equal(result.status, 0, scenario);
    assert.equal(result.stdout, scenario === "valid" ? "native=false\nweb=true\nnix=false\n" : "native=true\nweb=true\nnix=true\n", scenario);
    assert.equal(result.stderr, "", scenario);
  }
});

test("a classifier crash fails the configured workflow shell, even after partial output", { skip: process.platform === "win32" }, (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-ci-crash-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const { command, args } = workflowStep();
  for (const partial of ["", "native=false\\nweb=false\\nnix=false\\n"]) {
    const result = spawnSync("bash", [...args, "-c", `node() { printf '%b' "$PARTIAL"; return 17; }\n${command}`], {
      cwd: fileURLToPath(new URL("..", import.meta.url)), encoding: "utf8", timeout: 5000,
      env: { ...process.env, EVENT: "merge_group", RUNNER_TEMP: dir, GITHUB_OUTPUT: join(dir, "output"), PARTIAL: partial },
    });
    assert.equal(result.status, 17);
  }
});

function aptWorkflow(mirrors) {
  const workflow = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
  const step = workflow.match(/- name: Install system WebView libraries \(Linux\)\n([\s\S]*?)\n      - name:/)?.[1];
  assert.ok(step, "the Linux dependency step exists");
  return step.match(/        run: \|\n([\s\S]*)/)[1].replace(/^          /gm, "")
    .replace('Path("/etc/apt/apt-mirrors.txt")', `Path(${JSON.stringify(mirrors)})`);
}

const runnerMirrors = "http://azure.archive.ubuntu.com/ubuntu/\tpriority:1\nhttps://archive.ubuntu.com/ubuntu/\tpriority:2\nhttps://security.ubuntu.com/ubuntu/\tpriority:3\n";
const aptStubs = `sudo() { [[ "$1" == apt-get || "$1" == python3 ]] || return 98; "$@"; }
  apt-get() {
    printf '%s\\n' "$*" >> "$APT_LOG"
    if [[ " $* " == *" $FAIL_APT "* ]]; then return 42; fi
  }`;

test("Linux WebView downloads prefer the existing HTTPS archive, bound APT and propagate failures", { skip: process.platform === "win32" }, (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-ci-apt-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const log = join(dir, "calls");
  const mirrors = join(dir, "mirrors");
  const command = aptWorkflow(mirrors);
  for (const [failure, contents] of [["none", runnerMirrors], ["update", runnerMirrors], ["install", runnerMirrors], ["none", runnerMirrors.replace("priority:2", "priority:0")]]) {
    writeFileSync(log, "");
    writeFileSync(mirrors, contents);
    const result = spawnSync("bash", ["--noprofile", "--norc", "-e", "-o", "pipefail", "-c", `${aptStubs}\n${command}`], {
      cwd: dir, encoding: "utf8", timeout: 5000, env: { ...process.env, APT_LOG: log, FAIL_APT: failure },
    });
    assert.equal(result.status, failure === "none" ? 0 : 42, result.stderr);
    assert.equal(readFileSync(mirrors, "utf8"), runnerMirrors.replace("priority:2", "priority:0"), "only the already configured archive priority changes");
    const calls = readFileSync(log, "utf8").trim().split("\n");
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
  const dir = mkdtempSync(join(tmpdir(), "ibis-ci-apt-refusal-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const mirrors = join(dir, "mirrors"), log = join(dir, "calls");
  for (const contents of ["", runnerMirrors.replace("https://archive.ubuntu.com/ubuntu/", "https://other.example/ubuntu/"), runnerMirrors + "https://archive.ubuntu.com/ubuntu/\tpriority:2\n", runnerMirrors.replace("priority:2", "priority:9")]) {
    writeFileSync(mirrors, contents);
    writeFileSync(log, "");
    const result = spawnSync("bash", ["-e", "-o", "pipefail", "-c", `${aptStubs}\n${aptWorkflow(mirrors)}`], {
      cwd: dir, encoding: "utf8", timeout: 5000, env: { ...process.env, APT_LOG: log, FAIL_APT: "none" },
    });
    assert.equal(result.status, 1);
    assert.equal(readFileSync(log, "utf8"), "");
    assert.equal(readFileSync(mirrors, "utf8"), contents);
  }
});

test("only the Linux offline observer is elevated, and capture keeps failures", { skip: process.platform === "win32" }, (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-ci-observer-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  mkdirSync(join(dir, "target/debug"), { recursive: true });
  const app = join(dir, "target/debug/ibis-desktop");
  writeFileSync(app, "#!/bin/sh\nexec sleep 30\n");
  chmodSync(app, 0o755);
  const workflow = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
  const step = workflow.match(/- name: Capture the picker \(Linux and macOS\)\n([\s\S]*?)\n      - name:/)[1];
  const command = step.match(/        run: \|\n([\s\S]*)/)[1].replace(/^          /gm, "").split('\nif [ "$RUNNER_OS" = Linux ]; then')[0];
  assert.match(command, /^  target\/debug\/ibis-desktop > screenshots\/startup-/m, "the app stays unprivileged");
  const stubs = `python3() {
    printf '%s\n' "python $OBSERVER_ELEVATED $*" >> "$CALLS"
    if [[ "$*" == *--pid* ]]; then
      [[ "$RUNNER_OS" != Linux || "$OBSERVER_ELEVATED" == 1 ]] || return 41
      return "$SAMPLE_STATUS"
    fi
  }
  sudo() {
    [[ "$1" == -n && "$2" == python3 ]] || return 98
    printf 'sudo\n' >> "$CALLS"
    [[ "$SUDO_STATUS" == 0 ]] || return "$SUDO_STATUS"
    shift; OBSERVER_ELEVATED=1 "$@"
  }
  import() { :; }; screencapture() { :; }`;
  const calls = join(dir, "calls");
  for (const [system, sample, sudo, expected] of [["Linux", 0, 0, 0], ["Linux", 42, 0, 42], ["Linux", 0, 73, 73], ["macOS", 0, 0, 0], ["macOS", 42, 0, 42]]) {
    writeFileSync(calls, "");
    const result = spawnSync("bash", ["-e", "-o", "pipefail", "-c", `${stubs}\n${command}\ncapture`], {
      cwd: dir, encoding: "utf8", timeout: 5000,
      env: { ...process.env, RUNNER_OS: system, CALLS: calls, OBSERVER_ELEVATED: "0", SAMPLE_STATUS: String(sample), SUDO_STATUS: String(sudo) },
    });
    assert.equal(result.status, expected, `${system}: ${result.stderr}`);
    const log = readFileSync(calls, "utf8");
    assert.match(log, /^python 0 scripts\/check-offline.py --record-baseline /);
    assert.equal(log.includes("sudo\n"), system === "Linux");
    if (sudo === 0) assert.match(log, new RegExp(`python ${system === "Linux" ? 1 : 0} scripts/check-offline.py --pid \\d+ --baseline screenshots/offline-${system}.json --seconds 60`));
  }
});
