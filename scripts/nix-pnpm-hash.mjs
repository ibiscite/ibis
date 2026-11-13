#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

function validHash(hash) {
  return /^sha256-[A-Za-z0-9+/]{43}=$/.test(hash)
    && Buffer.from(hash.slice(7), "base64").toString("base64") === hash.slice(7);
}

/** Apply only the SHA-256 reported by Nix, never arbitrary build-log text. */
export function updateHash(hash, path = new URL("../nix/pnpm-deps-hash", import.meta.url)) {
  if (!validHash(hash)) throw new Error("Expected the got: SHA-256 SRI hash (sha256-…=).");
  writeFileSync(path, `${hash}\n`);
}

/** CI prints a copyable command; it never rewrites a pull request's files. */
export function hashFix(log) {
  const mismatches = log.matchAll(/hash mismatch in fixed-output derivation '([^'\r\n]+)':\s+specified:\s+\S+\s+got:\s+(\S+)/g);
  for (const [, path, hash] of mismatches) {
    if (path.endsWith("-ibis-desktop-pnpm-deps.drv") && validHash(hash)) {
      return `node scripts/nix-pnpm-hash.mjs '${hash}'`;
    }
  }
  return null;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const args = process.argv.slice(2);
    if (args.length === 2 && args[0] === "--from-log") {
      const fix = hashFix(readFileSync(args[1], "utf8"));
      if (fix) console.log(fix);
    } else if (args.length === 1) {
      updateHash(args[0]);
      console.log("Updated nix/pnpm-deps-hash; include it with the pnpm lock change.");
    } else {
      throw new Error("Usage: node scripts/nix-pnpm-hash.mjs HASH | --from-log BUILD_LOG");
    }
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
