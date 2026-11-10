#!/usr/bin/env node
// The MCP Registry entry for `ibis mcp` (docs/mcp-registry.md). Nothing here
// publishes: the maintainer runs `mcp-publisher` with the output.
//   node scripts/mcp-registry.mjs manifest SYSTEM    # the bundle's manifest.json
//   node scripts/mcp-registry.mjs server DIR         # server.json for DIR's bundles
// Each release carries one bundle per system, `ibis-mcp-<system>.mcpb`: the
// `ibis` command and this manifest, zipped. The registry needs each one's SHA-256.
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const read = (path) => readFileSync(join(root, path), "utf8");

export const NAME = "io.github.ibiscite/ibis";
export const SCHEMA = "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json";
/** Bundle systems and the MCPB platform each runs on. macOS is one universal binary. */
export const SYSTEMS = { macOS: "darwin", Windows: "win32", Linux: "linux" };

export const version = () => read("version.txt").trim();
export const bundle = (system, tag = `v${version()}`) =>
  `https://github.com/ibiscite/ibis/releases/download/${tag}/ibis-mcp-${system}.mcpb`;

/** The manifest for one system's bundle: the shared one, run only there. */
export function manifest(system) {
  const platform = SYSTEMS[system];
  if (!platform) throw new Error(`system must be one of: ${Object.keys(SYSTEMS).join(", ")}`);
  const shared = JSON.parse(read("plugins/ibis/mcpb/manifest.json"));
  return { ...shared, compatibility: { ...shared.compatibility, platforms: [platform] } };
}

/** The registry entry, with each bundle's SHA-256 as `{ system: hex }`. */
export function server(hashes) {
  const shared = JSON.parse(read("plugins/ibis/mcpb/manifest.json"));
  const systems = Object.keys(hashes);
  if (systems.length === 0 || systems.some((system) => !SYSTEMS[system])) {
    throw new Error(`bundles must be named ibis-mcp-<${Object.keys(SYSTEMS).join("|")}>.mcpb`);
  }
  return {
    $schema: SCHEMA,
    name: NAME,
    title: shared.display_name,
    description: "Search your Zotero or BibTeX library and cite real records, read-only and offline.",
    version: version(),
    websiteUrl: shared.homepage,
    repository: { url: shared.repository.url, source: "github" },
    packages: systems.map((system) => ({
      registryType: "mcpb",
      identifier: bundle(system),
      fileSha256: hashes[system],
      transport: { type: "stdio" },
    })),
  };
}

function hashes(folder) {
  const found = {};
  for (const file of readdirSync(folder)) {
    const system = /^ibis-mcp-(.+)\.mcpb$/.exec(file)?.[1];
    if (system) found[system] = createHash("sha256").update(readFileSync(join(folder, file))).digest("hex");
  }
  return found;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [command, argument] = process.argv.slice(2);
  const output = command === "manifest" ? manifest(argument) : command === "server" && argument ? server(hashes(argument)) : null;
  if (!output) {
    console.error("usage: node scripts/mcp-registry.mjs manifest SYSTEM | server DIR");
    process.exit(2);
  }
  console.log(JSON.stringify(output, null, 2));
}
