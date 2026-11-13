import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { hashFix, updateHash } from "./nix-pnpm-hash.mjs";

const hash = `sha256-${Buffer.alloc(32, 1).toString("base64")}`;
const script = fileURLToPath(new URL("./nix-pnpm-hash.mjs", import.meta.url));
const mismatch = (name, got = hash) => `error: hash mismatch in fixed-output derivation '/nix/store/example-${name}.drv':
         specified: sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
            got:    ${got}
`;

test("a reported SHA-256 replaces the dependency hash with one newline", (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-nix-hash-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const path = join(dir, "pnpm-deps-hash");
  writeFileSync(path, "old hash\n");
  updateHash(hash, path);
  assert.equal(readFileSync(path, "utf8"), `${hash}\n`);
  updateHash(hash, path);
  assert.equal(readFileSync(path, "utf8"), `${hash}\n`, "repeating the fix is harmless");
});

test("invalid or ambiguous hashes leave the existing file intact", (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-nix-hash-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const path = join(dir, "pnpm-deps-hash");
  writeFileSync(path, "unchanged\n");
  for (const value of ["", "sha256-short", hash.replace("sha256", "sha512"), `${hash}\n${hash}`, `${hash}; false`, hash.replace("E=", "F=")]) {
    assert.throws(() => updateHash(value, path), /SHA-256/);
    assert.equal(readFileSync(path, "utf8"), "unchanged\n");
  }
});

test("a failed write is reported to the caller", () => {
  assert.throws(() => updateHash(hash, new URL("./missing-directory/pnpm-deps-hash", import.meta.url)), /ENOENT/);
});

test("a pnpm mismatch prints the exact fix, ignoring other fixed-output hashes", () => {
  assert.equal(hashFix(mismatch("pnpm-11.25.0") + mismatch("ibis-desktop-pnpm-deps")), `node scripts/nix-pnpm-hash.mjs '${hash}'`);
  for (const log of ["compiler error", mismatch("pnpm-11.25.0"), mismatch("ibis-desktop-pnpm-deps", "invalid")]) {
    assert.equal(hashFix(log), null);
  }
});

test("CI log inspection prints the fix without changing the hash file", (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-nix-hash-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const log = join(dir, "build.log");
  const hashFile = new URL("../nix/pnpm-deps-hash", import.meta.url);
  const before = readFileSync(hashFile, "utf8");
  writeFileSync(log, mismatch("ibis-desktop-pnpm-deps"));
  const result = spawnSync(process.execPath, [script, "--from-log", log], { encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout.trim(), `node scripts/nix-pnpm-hash.mjs '${hash}'`);
  assert.equal(readFileSync(hashFile, "utf8"), before);
});

test("CLI misuse and an unreadable build log fail with an actionable message", () => {
  for (const args of [[], ["invalid"], [hash, hash], ["--from-log", `${script}.missing`]]) {
    const result = spawnSync(process.execPath, [script, ...args], { encoding: "utf8" });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Usage:|SHA-256|ENOENT/);
  }
});
