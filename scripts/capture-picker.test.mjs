import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { nightlyConfig } from "./update-feed.mjs";

// The observer, sudo, the display and screenshots are replaced; the script,
// its trap and the shell that runs it are real. Each stub is written before
// anything runs: rewriting a running executable fails with ETXTBSY.
const stubs = {
  python3: `printf 'python %s %s\\n' "$OBSERVER_ELEVATED" "$*" >> "$CALLS"
if [[ "$*" == *--pid* ]]; then
  cat "$APP_STARTED" >> "$CALLS" # Waits until the app has started.
  [[ "$RUNNER_OS" != Linux || "$OBSERVER_ELEVATED" == 1 ]] || exit 41
  exit "$SAMPLE_STATUS"
fi`,
  sudo: `[[ "$1" == -n && "$2" == python3 ]] || exit 98
printf 'sudo %s\\n' "$*" >> "$CALLS"
[[ "$SUDO_STATUS" == 0 ]] || exit "$SUDO_STATUS"
shift; OBSERVER_ELEVATED=1 exec "$@"`,
  "xvfb-run": `printf 'display\\n' >> "$CALLS"
[[ "$1" == -a && "$2" == -s ]] || exit 97
shift 3; exec "$@"`,
  "dbus-run-session": `[[ "$1" == -- ]] || exit 96
shift; exec "$@"`,
  import: `printf 'screenshot %s\\n' "$*" >> "$CALLS"`,
  screencapture: `printf 'screenshot %s\\n' "$*" >> "$CALLS"`,
};

test("only the Linux offline observer is elevated, and capture keeps failures", { skip: process.platform === "win32" }, (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-capture-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  mkdirSync(join(dir, "bin"));
  for (const [name, body] of Object.entries(stubs)) writeFileSync(join(dir, "bin", name), `#!/usr/bin/env bash\n${body}\n`, { mode: 0o755 });
  const config = join(dir, "nightly.conf.json");
  writeFileSync(config, JSON.stringify(nightlyConfig("0.1.2-nightly.20261028000000", "linux")));
  const cases = [["Linux", 0, 0, 0], ["Linux", 42, 0, 42], ["Linux", 0, 73, 73], ["macOS", 0, 0, 0], ["macOS", 42, 0, 42]];
  // The EXIT trap signals each app without waiting for it, so every case has
  // its own app and start signal.
  const runs = cases.map(([system], index) => {
    const target = `synthetic-${index}`;
    mkdirSync(join(dir, `target/${target}/release`), { recursive: true });
    const started = join(dir, `started-${index}`);
    assert.equal(spawnSync("mkfifo", [started]).status, 0);
    const app = join(dir, `target/${target}/release/${system === "Linux" ? "ibis-nightly" : "ibis-desktop"}`);
    writeFileSync(app, `#!/bin/sh\nprintf 'app %s\\n' "$OBSERVER_ELEVATED" > '${started}'\nexec sleep 30\n`, { mode: 0o755 });
    return { target, started };
  });
  for (const [index, [system, sample, sudo, expected]] of cases.entries()) {
    const calls = join(dir, `calls-${index}`);
    writeFileSync(calls, "");
    const result = spawnSync(fileURLToPath(new URL("./capture-picker.sh", import.meta.url)), [], {
      cwd: dir, encoding: "utf8", timeout: 5000,
      env: { ...process.env, PATH: `${join(dir, "bin")}${delimiter}${process.env.PATH}`, TARGET: runs[index].target, CONFIG: system === "Linux" ? config : "", RUNNER_OS: system, CALLS: calls, APP_STARTED: runs[index].started, OBSERVER_ELEVATED: "0", SAMPLE_STATUS: String(sample), SUDO_STATUS: String(sudo) },
    });
    const log = readFileSync(calls, "utf8");
    assert.equal(result.status, expected, `${system}: ${result.stderr}\n${log}`);
    const lines = log.trim().split("\n");
    assert.equal(lines[0], system === "Linux" ? "display" : `python 0 scripts/check-offline.py --record-baseline screenshots/offline-${system}.json`);
    assert.ok(lines.includes(`python 0 scripts/check-offline.py --record-baseline screenshots/offline-${system}.json`));
    assert.equal(log.includes("sudo "), system === "Linux");
    if (system === "Linux") assert.match(log, /^sudo -n python3 scripts\/check-offline\.py --pid \d+ /m, "sudo runs the observer and nothing else");
    if (sudo === 0) {
      assert.match(log, new RegExp(`^python ${system === "Linux" ? 1 : 0} scripts/check-offline.py --pid \\d+ --baseline screenshots/offline-${system}.json --seconds 60$`, "m"));
      assert.ok(lines.includes("app 0"), "the release app stays unprivileged");
    }
    assert.equal(log.includes("screenshot "), expected === 0, "a screenshot only after the app stayed up");
  }
});
