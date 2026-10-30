import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { classify } from "./ci-paths.mjs";
import { nightlyConfig } from "./update-feed.mjs";

const full = { native: true, web: true };
const row = (filename, previous_filename = null) => ({ filename, previous_filename, status: previous_filename ? "renamed" : "modified" });
const pr = (files, expected = files.length) => classify("pull_request", expected, files);

function workflowStep() {
  const workflow = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
  const step = workflow.match(/id: classify\n([\s\S]*?)\n\n/)[1];
  const shell = step.match(/^        shell: (\S+)$/m)?.[1];
  assert.ok(shell === undefined || shell === "bash");
  // GitHub adds pipefail only for an explicitly selected bash shell.
  return { command: step.match(/        run: \|\n([\s\S]*)/)[1].replace(/^          /gm, ""), args: shell === "bash" ? ["-e", "-o", "pipefail"] : ["-e"] };
}

test("interface-only changes retain web checks without native jobs", () => {
  for (const path of ["apps/desktop/src/App.tsx", "apps/desktop/src/features/picker/PickerFooter.css", "apps/desktop/src/messages.ts", "apps/desktop/src/icon.svg", "apps/desktop/index.html"]) {
    assert.deepEqual(pr([row(path), row("docs/design.md")]), { native: false, web: true }, path);
  }
});

test("generated bindings always run native contract checks, including renames", () => {
  const bindings = "apps/desktop/src/bindings.ts";
  for (const status of ["added", "modified", "removed"]) {
    assert.deepEqual(pr([{ filename: bindings, status }]), full, status);
  }
  assert.deepEqual(pr([row("apps/desktop/src/renamed.ts", bindings)]), full);
  assert.deepEqual(pr([row(bindings, "apps/desktop/src/old.ts")]), full);
  assert.deepEqual(pr([row("apps/desktop/src/tokens.css")]), { native: false, web: true });
});

test("website-only changes keep web checks without native jobs", () => {
  for (const path of ["website/index.html", "website/site.js", "website/_headers"]) {
    assert.deepEqual(pr([row(path)]), { native: false, web: true }, path);
  }
  assert.deepEqual(pr([row("website/site.js"), row("apps/desktop/src-tauri/src/lib.rs")]), { native: true, web: true });
  assert.equal(pr([row("websites/site.js")]).native, true, "only the website directory itself");
});

test("without native inputs only a Linux Desktop placeholder runs", () => {
  const workflow = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
  const desktop = workflow.match(/\n  desktop:\n([\s\S]*?)\n    steps:/)[1];
  assert.match(desktop, /os: \$\{\{ needs\.changes\.outputs\.native == 'true' && fromJSON\('\["ubuntu-24\.04", "macos-15", "windows-2025"\]'\) \|\| fromJSON\('\["ubuntu-24\.04"\]'\) \}\}/);
});

test("only allowlisted documentation additions, edits and deletions take the fast path", () => {
  for (const status of ["added", "modified", "removed"]) {
    for (const filename of ["README.md", "ROADMAP.md", "docs/design/menu.md", "website/README.md", "LICENSE", ".github/ISSUE_TEMPLATE/bug_report.yml"]) {
      assert.deepEqual(pr([{ filename, status }]), { native: false, web: false }, `${status}: ${filename}`);
    }
  }
});

test("Markdown additions and deletions beside code and vendor assets require Rust checks", () => {
  for (const status of ["added", "modified", "removed"]) {
    for (const filename of ["crates/ibis-csl/vendor/README.md", "crates/ibis-csl/README.md", "fixtures/README.md", "apps/desktop/README.md", "vendor/README.md", "docs/vendor/README.md", "scripts/README.md", "unknown/README.md", "LICENSE/README.md"]) {
      assert.deepEqual(pr([{ filename, status }]), full, `${status}: ${filename}`);
    }
    assert.deepEqual(pr([{ filename: "website/vendor/README.md", status }]), { native: false, web: true });
  }
});

test("native, tooling and unknown changes stay conservative", () => {
  for (const path of ["crates/ibis-desktop/src/lib.rs", "Cargo.toml", "apps/desktop/vite.config.ts", "apps/desktop/src/unknown.rs", "apps/desktop/src-tauri/tauri.conf.json", "scripts/smoke-native.py", "scripts/update-e2e.py", ".github/workflows/ci.yml", "unknown/file", "docs/design-settings.json", "docs/design-settings.json.bak"]) {
    assert.deepEqual(pr([row("apps/desktop/src/App.tsx"), row(path)]), { native: true, web: true }, path);
  }
});

test("both sides of a rename determine the required checks", () => {
  assert.deepEqual(pr([row("apps/desktop/src/Renamed.tsx", "apps/desktop/src/App.tsx")]), { native: false, web: true });
  assert.equal(pr([row("docs/old.md", "crates/ibis-desktop/src/lib.rs")]).native, true);
  assert.deepEqual(pr([row("docs/vendor-notes.md", "crates/ibis-csl/vendor/README.md")]), full);
  assert.deepEqual(pr([row("crates/ibis-csl/vendor/README.md", "docs/vendor-notes.md")]), full);
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
  assert.deepEqual(pr([row("apps/desktop/src/研究.tsx"), row("apps/desktop/src/مرحبا.css"), row("docs/$(false)`x`.md")]), { native: false, web: true });
});

test("main and merge queue always validate the complete app", () => {
  for (const event of ["push", "merge_group", "unknown"]) {
    for (const status of ["added", "modified", "removed"]) {
      for (const filename of ["README.md", "docs/design.md", "crates/ibis-csl/vendor/README.md"]) {
        assert.deepEqual(classify(event, 1, [{ filename, status }]), full);
      }
    }
  }
  assert.deepEqual(classify("workflow_dispatch"), full);
  const workflow = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
  const push = workflow.match(/^  push:\n((?:    .*\n)*)/m)?.[1];
  assert.ok(push, "CI runs on pushes");
  assert.match(push, /branches: \[main\]/);
  assert.doesNotMatch(push, /\bpaths(?:-ignore)?:/, "main pushes must not filter out Markdown or other paths");
});

test("the CLI emits only fixed decisions for valid, missing and malformed metadata", (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-ci-paths-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const file = join(dir, "synthetic.ndjson");
  for (const [contents, expected] of [[JSON.stringify(row("apps/desktop/src/研究.tsx")), "native=false\nweb=true\n"], ["private-資料: invalid metadata", "native=true\nweb=true\n"], ["", "native=true\nweb=true\n"], [null, "native=true\nweb=true\n"]]) {
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

test("Desktop starts after classification and CI result retains the required jobs", () => {
  const workflow = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
  const changes = workflow.match(/\n  changes:\n([\s\S]*?)\n  test:/)[1];
  assert.match(changes, /pnpm exec vite build\n/);
  assert.match(changes, /- name: Share the web interface\n        if: steps\.classify\.outputs\.native == 'true'[\s\S]*name: desktop-web/);
  assert.match(workflow.match(/\n  desktop:\n([\s\S]*?)\n    steps:/)[1], /\n    needs: changes\n/);
  assert.match(workflow, /result:[\s\S]*needs: \[changes, test, web, desktop, fidelity, dependencies\]/, "CI result still needs every required job");
});

test("pull request CI gates search and picker size; Preview gates open and refresh", () => {
  const workflow = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
  const engine = workflow.match(/\n  test:\n([\s\S]*?)\n  web:/)[1];
  assert.match(engine, /os: \[ubuntu-24\.04\]/);
  assert.match(engine, /cargo test -p ibis-engine --locked --release --test refresh_budget -- search_budget --ignored --nocapture --test-threads=1/);
  assert.doesNotMatch(workflow, /-- open_refresh_budget|Enforce 100k open\/refresh/);
  assert.ok(engine.indexOf("Enforce 100k search budget") > engine.indexOf("run: cargo test --locked"));
  const preview = readFileSync(new URL("../.github/workflows/preview.yml", import.meta.url), "utf8");
  const build = preview.split("\n  build:\n")[1].split("\n  sign-macos:\n")[0];
  assert.match(build, /- name: Report or enforce 100k open\/refresh budgets \(Linux, release\)[\s\S]*?run: node scripts\/preview-budgets.mjs/);
  for (const build of [workflow.match(/\n  changes:\n([\s\S]*?)\n  test:/)[1], workflow.match(/\n  web:\n([\s\S]*?)\n  desktop:/)[1]]) {
    assert.match(build, /node ..\/..\/scripts\/picker-budget.mjs dist/);
  }
});

test("Rust and fidelity skip only documentation through the conservative path decision", () => {
  const workflow = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
  for (const job of ["test", "fidelity"]) {
    const config = workflow.match(new RegExp(`\\n  ${job}:\\n([\\s\\S]*?)\\n    steps:`))[1];
    assert.match(config, /\n    needs: changes\n    if: needs.changes.outputs.web == 'true'\n/);
  }
  assert.deepEqual(pr([row("ROADMAP.md")]), { native: false, web: false });
  assert.deepEqual(pr([row("ROADMAP.md", "crates/ibis-core/src/lib.rs")]), full);
  assert.match(workflow, /cargo fmt --all --check/);
  assert.match(workflow, /os: .*ubuntu-24\.04.*macos-15.*windows-2025/);
  assert.match(workflow, /cargo clippy --all-targets --locked\n          cargo test --locked/);
  assert.match(workflow, /cargo test -p ibis-desktop --locked/);
  assert.match(workflow, /- name: Exercise the native picker \(Linux\)/);
  assert.match(workflow, /- name: Test the Windows offline observer\n        if: env.NATIVE == 'true' && runner.os == 'Windows'\n        shell: pwsh\n        run: \.\/scripts\/check-offline.tests.ps1/);
});

test("CI result accepts only deliberate skips and rejects failures, cancellation and missing decisions", { skip: process.platform === "win32" }, () => {
  const workflow = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
  const resultJob = workflow.split("\n  result:\n")[1];
  assert.match(resultJob, /\n    if: always\(\)\n/);
  assert.match(resultJob, /RESULTS: \$\{\{ needs.changes.result \}\} \$\{\{ needs.web.result \}\} \$\{\{ needs.desktop.result \}\} \$\{\{ needs.dependencies.result \}\}/);
  assert.match(resultJob, /WEB: \$\{\{ needs.changes.outputs.web \}\}/);
  assert.match(resultJob, /NATIVE: \$\{\{ needs.changes.outputs.native \}\}/);
  assert.match(resultJob, /RUST_RESULTS: \$\{\{ needs.test.result \}\} \$\{\{ needs.fidelity.result \}\}/);
  const command = resultJob.match(/        run: \|\n([\s\S]*)/)[1].replace(/^          /gm, "");
  const check = (overrides, expected) => {
    const result = spawnSync("bash", ["-e", "-o", "pipefail", "-c", command], {
      encoding: "utf8", timeout: 5000,
      env: { ...process.env, EVENT: "pull_request", BASE: "main", WEB: "true", NATIVE: "true", RESULTS: "success success success success", RUST_RESULTS: "success success", ...overrides },
    });
    assert.equal(result.status, expected, `${JSON.stringify(overrides)}: ${result.stdout}${result.stderr}`);
  };
  check({}, 0);
  for (const status of ["added", "removed"]) {
    const selection = pr([{ filename: "crates/ibis-csl/vendor/README.md", status }]);
    check({ WEB: String(selection.web), NATIVE: String(selection.native), RUST_RESULTS: "skipped skipped" }, 1);
  }
  check({ WEB: "false", NATIVE: "false", RUST_RESULTS: "skipped skipped" }, 0);
  check({ NATIVE: "false" }, 0); // Interface/website changes still run Rust.
  check({ EVENT: "push", BASE: "" }, 0);
  check({ BASE: "feature" }, 1);
  for (const web of ["", "unknown"]) check({ WEB: web }, 1);
  for (const native of ["", "unknown"]) check({ NATIVE: native }, 1);
  check({ WEB: "false", RUST_RESULTS: "skipped skipped" }, 1);
  check({ EVENT: "push", WEB: "false", NATIVE: "false", RUST_RESULTS: "skipped skipped" }, 1);
  check({ EVENT: "push", NATIVE: "false" }, 1);
  for (const state of ["failure", "cancelled", "skipped", ""]) {
    for (let i = 0; i < 4; i++) {
      const results = ["success", "success", "success", "success"];
      results[i] = state;
      check({ RESULTS: results.join(" ") }, 1);
    }
    for (const results of [`${state} success`, `success ${state}`]) check({ RUST_RESULTS: results }, 1);
  }
  for (const state of ["failure", "cancelled", "success", ""]) {
    for (const results of [`${state} skipped`, `skipped ${state}`]) check({ WEB: "false", NATIVE: "false", RUST_RESULTS: results }, 1);
  }
});

test("slow checks run in the existing nightly/release workflow, never on pull requests", () => {
  const ci = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
  const preview = readFileSync(new URL("../.github/workflows/preview.yml", import.meta.url), "utf8");
  const withoutEngineGate = ci.replace("cargo test -p ibis-engine --locked --release --test refresh_budget -- search_budget --ignored --nocapture --test-threads=1", "");
  assert.doesNotMatch(withoutEngineGate, /--release|toolchain install|--seconds 60|-Seconds 60|real-zotero.sh/);
  assert.doesNotMatch(preview, /^  pull_request:/m);
  assert.match(preview, /^  schedule:\n    - cron:/m);
  const build = preview.split("\n  build:\n")[1].split("\n  sign-macos:\n")[0];
  assert.match(build, /if: github.event_name != 'schedule' && !inputs.update_test/);
  assert.match(build, /- name: Test core and sources in release mode \(Linux\)\n        if: runner.os == 'Linux'\n        env:\n          RUSTFLAGS: -D warnings\n        run: cargo test -p ibis-core -p ibis-sources --release --locked/);
  assert.match(build, /rustup toolchain install 1\.90\.0 --profile minimal\n          cargo \+1\.90\.0 test --locked/);
  assert.match(build, /- name: Capture the picker \(Linux and macOS\)\n        if: runner.os != 'Windows' && matrix.target != 'x86_64-apple-darwin'/);
  assert.match(build, /--seconds 60/);
  assert.match(build, /Start-Process "target\/\$env:TARGET\/release\/ibis-desktop.exe"/);
  assert.match(build, /-Seconds 60/);
  assert.match(preview, /\n  zotero:\n    name: Real Zotero\n    if: \$\{\{ !inputs.update_test \}\}/);
  assert.match(preview, /inputs:\n[\s\S]*?zotero_capture_version:/);
  assert.match(preview, /run: scripts\/real-zotero.sh --capture/);
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
    assert.equal(result.stdout, scenario === "valid" ? "native=false\nweb=true\n" : "native=true\nweb=true\n", scenario);
    assert.equal(result.stderr, "", scenario);
  }
});

test("a classifier crash fails the configured workflow shell, even after partial output", { skip: process.platform === "win32" }, (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-ci-crash-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const { command, args } = workflowStep();
  for (const partial of ["", "native=false\\nweb=false\\n"]) {
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
  mkdirSync(join(dir, "target/synthetic/release"), { recursive: true });
  const config = join(dir, "nightly.conf.json");
  writeFileSync(config, JSON.stringify(nightlyConfig("0.1.2-nightly.20261028000000", "linux")));
  const workflow = readFileSync(new URL("../.github/workflows/preview.yml", import.meta.url), "utf8");
  const step = workflow.match(/- name: Capture the picker \(Linux and macOS\)\n([\s\S]*?)\n      - name:/)[1];
  const command = step.match(/        run: \|\n([\s\S]*)/)[1].replace(/^          /gm, "").split('\nif [ "$RUNNER_OS" = Linux ]; then')[0];
  assert.match(command, /^  "target\/\$TARGET\/release\/\$binary" > screenshots\/startup-/m, "the release app stays unprivileged");
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
    const app = join(dir, `target/synthetic/release/${system === "Linux" ? "ibis-nightly" : "ibis-desktop"}`);
    writeFileSync(app, "#!/bin/sh\nexec sleep 30\n");
    chmodSync(app, 0o755);
    writeFileSync(calls, "");
    const result = spawnSync("bash", ["-e", "-o", "pipefail", "-c", `${stubs}\n${command}\ncapture`], {
      cwd: dir, encoding: "utf8", timeout: 5000,
      env: { ...process.env, TARGET: "synthetic", CONFIG: system === "Linux" ? config : "", RUNNER_OS: system, CALLS: calls, OBSERVER_ELEVATED: "0", SAMPLE_STATUS: String(sample), SUDO_STATUS: String(sudo) },
    });
    assert.equal(result.status, expected, `${system}: ${result.stderr}`);
    const log = readFileSync(calls, "utf8");
    assert.match(log, /^python 0 scripts\/check-offline.py --record-baseline /);
    assert.equal(log.includes("sudo\n"), system === "Linux");
    if (sudo === 0) assert.match(log, new RegExp(`python ${system === "Linux" ? 1 : 0} scripts/check-offline.py --pid \\d+ --baseline screenshots/offline-${system}.json --seconds 60`));
  }
});

test("fuzz driver uses stable while the actual sanitizer campaign stays on nightly", () => {
  const preview = readFileSync(new URL("../.github/workflows/preview.yml", import.meta.url), "utf8");
  const fuzz = preview.split("\n  bibtex-fuzz:\n")[1].split("\n  zotero:\n")[0];
  assert.match(fuzz, /cargo \+stable install cargo-fuzz --version 0\.13\.1 --locked/);
  assert.doesNotMatch(fuzz, /cargo \+nightly install cargo-fuzz/);
  assert.match(fuzz, /workflow_dispatch.*!inputs.update_test/);
  assert.match(fuzz, /cargo \+nightly fuzz build damaged_bibtex --features fuzzing --target-dir target\/fuzz/);
  assert.match(fuzz, /cargo \+nightly fuzz run damaged_bibtex --features fuzzing --target-dir target\/fuzz -- -max_total_time=300 -timeout=5 -max_len=65536 -rss_limit_mb=2048/);
});
