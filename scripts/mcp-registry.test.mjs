import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { NAME, SYSTEMS, bundle, manifest, server, version } from "./mcp-registry.mjs";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");

test("the registry entry meets the registry's limits and names each release bundle", () => {
  const hex = "0123456789abcdef".repeat(4);
  const entry = server({ macOS: hex, Windows: hex, Linux: hex });
  assert.match(entry.name, /^[a-zA-Z0-9.-]+\/[a-zA-Z0-9._-]+$/);
  assert.equal(entry.name, NAME);
  assert.ok(entry.description.length >= 1 && entry.description.length <= 100, entry.description);
  assert.ok(entry.title.length <= 100);
  assert.equal(entry.version, version());
  for (const item of entry.packages) {
    assert.equal(item.registryType, "mcpb");
    assert.match(item.identifier, /^https:\/\/github\.com\/ibiscite\/ibis\/releases\/download\/v[^/]+\/ibis-mcp-[A-Za-z]+\.mcpb$/);
    assert.match(item.fileSha256, /^[a-f0-9]{64}$/);
    assert.deepEqual(item.transport, { type: "stdio" });
  }
  assert.throws(() => server({}), /bundles must be named/);
  assert.throws(() => server({ Amiga: hex }), /bundles must be named/);
});

test("each bundle runs `ibis mcp` on its own system and carries the release version", () => {
  for (const [system, platform] of Object.entries(SYSTEMS)) {
    const shown = manifest(system);
    assert.deepEqual(shown.compatibility.platforms, [platform]);
    assert.equal(shown.version, version());
    assert.deepEqual(shown.server.mcp_config.args, ["mcp", "--app-libraries"]);
  }
  assert.throws(() => manifest("Amiga"), /system must be one of/);
  const files = JSON.parse(read("release-please-config.json")).packages["."]["extra-files"];
  assert.ok(files.some((file) => file.path === "plugins/ibis/mcpb/manifest.json" && file.jsonpath === "$.version"));
});

test("server hashes the bundles it finds and ignores other files", () => {
  const folder = mkdtempSync(join(tmpdir(), "ibis-registry-"));
  try {
    writeFileSync(join(folder, "ibis-mcp-Linux.mcpb"), "invented bundle");
    writeFileSync(join(folder, "ibis-Linux.tar.gz"), "not a bundle");
    const run = spawnSync(process.execPath, [new URL("./mcp-registry.mjs", import.meta.url).pathname, "server", folder], { encoding: "utf8" });
    assert.equal(run.status, 0, run.stderr);
    const entry = JSON.parse(run.stdout);
    assert.deepEqual(entry.packages.map((item) => item.identifier), [bundle("Linux")]);
    assert.equal(entry.packages[0].fileSha256, createHash("sha256").update("invented bundle").digest("hex"));
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
});
