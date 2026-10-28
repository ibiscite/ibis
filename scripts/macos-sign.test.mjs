// The requirement check in scripts/macos-sign.sh refuses any app that is not
// the expected Ibis signed with the pinned certificate (#1242). codesign and
// plutil are stubbed; the shell under test is the real script.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("./macos-sign.sh", import.meta.url));
const leaf = "0123456789abcdef0123456789abcdef01234567";

function check({ bundle, designated }, identifier = "com.ibiscite.ibis.nightly") {
  const dir = mkdtempSync(join(tmpdir(), "ibis-sign-"));
  try {
    writeFileSync(join(dir, "plutil"), `#!/bin/sh\necho '${bundle}'\n`);
    // codesign -d -r- writes the requirement to stderr, after other lines.
    writeFileSync(join(dir, "codesign"), `#!/bin/sh\necho 'Executable=/x' >&2\necho '${designated}' >&2\n`);
    for (const tool of ["plutil", "codesign"]) chmodSync(join(dir, tool), 0o755);
    return spawnSync("bash", [script, "requirement", "Ibis Nightly.app", identifier, leaf], {
      encoding: "utf8",
      env: { ...process.env, PATH: `${dir}:${process.env.PATH}` },
    });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const pinned = `designated => identifier "com.ibiscite.ibis.nightly" and certificate leaf = H"${leaf}"`;

test("the pinned certificate and identifier pass", { skip: process.platform === "win32" }, () => {
  const result = check({ bundle: "com.ibiscite.ibis.nightly", designated: pinned });
  assert.equal(result.status, 0, result.stderr);
});

test("another certificate, an ad-hoc hash or another app is refused", { skip: process.platform === "win32" }, () => {
  for (const [what, app] of [
    ["another certificate", { bundle: "com.ibiscite.ibis.nightly", designated: pinned.replace(leaf, "f".repeat(40)) }],
    ["an ad-hoc build", { bundle: "com.ibiscite.ibis.nightly", designated: 'designated => cdhash H"8d967bc8"' }],
    ["an unsigned build", { bundle: "com.ibiscite.ibis.nightly", designated: "code object is not signed at all" }],
    ["stable signed as nightly", { bundle: "com.ibiscite.ibis", designated: pinned }],
  ]) {
    const result = check(app);
    assert.notEqual(result.status, 0, what);
    assert.match(result.stderr, /expected/, what);
  }
});
