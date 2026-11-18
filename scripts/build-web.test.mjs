import assert from "node:assert/strict";
import { test } from "node:test";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

test("failed web dependency setup stops the Windows paste check before building", { skip: process.platform === "win32" }, (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-windows-setup-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  mkdirSync(join(dir, "apps/desktop"), { recursive: true });
  writeFileSync(join(dir, "pnpm"), '#!/bin/sh\ncase "$1" in install) exit 23 ;; *) touch "$BUILD_MARKER"; exit 0 ;; esac\n', { mode: 0o755 });
  const marker = join(dir, "built");
  const result = spawnSync("bash", [fileURLToPath(new URL("./build-web.sh", import.meta.url))], { cwd: dir, encoding: "utf8", timeout: 5000, env: { ...process.env, PATH: `${dir}:${process.env.PATH}`, BUILD_MARKER: marker } });
  assert.equal(result.status, 23, result.stderr);
  assert.equal(existsSync(marker), false, "a failed install must not be consumed by a later build");
});
