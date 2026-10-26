import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { NIGHTLY_PUBKEY, PACKAGES, compareVersions, feedKeyId, feedPublicKey, keyId, manifest, nightlyConfig, nightlyVersion, previousNightlyIdentity } from "./update-feed.mjs";

const fixture = (name) => readFileSync(new URL(`../fixtures/updates/${name}`, import.meta.url), "utf8");
const testKey = fixture("test-key.pub");
const signature = fixture("package-versioned.bin.sig");
const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === "path") ?? "PATH";

test("previous identity lookup reads actual published and current source", () => {
  assert.equal(previousNightlyIdentity(fixture("previous-nightly/update-feed.mjs.txt")), "io.github.ibiscite.ibis.nightly");
  assert.equal(previousNightlyIdentity(readFileSync(new URL("./update-feed.mjs", import.meta.url), "utf8")), nightlyConfig("0.1.2-nightly.20261026000000").identifier);
});

test("the real previous-nightly workflow never blocks publication on unavailable identity evidence", () => {
  const workflow = readFileSync(new URL("../.github/workflows/preview.yml", import.meta.url), "utf8");
  const step = workflow.split("      - name: Keep the previous nightly for the update test\n")[1]?.split("      - name:")[0];
  assert.ok(step, "find the publishing step");
  const shell = step.split("        run: |\n")[1].split("\n").map((line) => line.replace(/^          /, "")).join("\n");
  for (const mode of ["published", "current", "source-error", "unknown", "moved", "tag-error", "no-feed"]) {
    const dir = mkdtempSync(join(tmpdir(), "ibis-previous-"));
    try {
      mkdirSync(join(dir, "scripts"));
      copyFileSync(new URL("./update-feed.mjs", import.meta.url), join(dir, "scripts/update-feed.mjs"));
      // Only GitHub is substituted. Execute the workflow shell and real CLI/source.
      writeFileSync(join(dir, "gh"), `#!/usr/bin/env node
const fs = require("node:fs");
const args = process.argv.slice(2).join(" ");
const mode = process.env.TEST_MODE;
if (args.includes("release download")) {
  if (mode === "no-feed") process.exit(1);
  fs.writeFileSync("previous/latest.json", "{}");
} else if (args.includes("contents/scripts/update-feed.mjs")) {
  if (mode === "source-error") process.exit(1);
  const source = mode === "current" ? fs.readFileSync(process.env.CURRENT_SOURCE) : mode === "unknown" ? Buffer.from("unknown source") : fs.readFileSync(process.env.PUBLISHED_SOURCE);
  console.log(source.toString("base64"));
} else if (args.includes("git/ref/tags/nightly")) {
  if (mode === "tag-error") process.exit(1);
  const seen = fs.existsSync("tag-read");
  fs.writeFileSync("tag-read", "");
  console.log((mode === "moved" && seen ? "b" : "a").repeat(40));
} else process.exit(2);
`, { mode: 0o755 });
      const result = spawnSync("bash", ["-e", "-o", "pipefail", "-c", shell], {
        cwd: dir, encoding: "utf8", timeout: 30000, env: { ...process.env, [pathKey]: `${dir}${delimiter}${process.env[pathKey] ?? ""}`, RUNNER_TEMP: dir,
          REPO: "example/ibis", TEST_MODE: mode,
          CURRENT_SOURCE: fileURLToPath(new URL("./update-feed.mjs", import.meta.url)),
          PUBLISHED_SOURCE: fileURLToPath(new URL("../fixtures/updates/previous-nightly/update-feed.mjs.txt", import.meta.url)) },
      });
      assert.equal(result.status, 0, `${mode}: ${result.stdout}${result.stderr}`);
      if (mode !== "no-feed") {
        const identity = readFileSync(join(dir, "previous/identifier.txt"), "utf8").trim();
        assert.equal(identity, mode === "published" ? "io.github.ibiscite.ibis.nightly" : mode === "current" ? "com.ibiscite.ibis.nightly" : "unavailable", mode);
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }
});

// Tauri CLI merges config overrides with RFC 7396 (json_patch::merge).
function mergeConfig(base, patch) {
  const result = structuredClone(base);
  for (const [key, value] of Object.entries(patch)) {
    if (value === null) delete result[key];
    else result[key] = typeof value === "object" && !Array.isArray(value)
      ? mergeConfig(result[key] ?? {}, value) : value;
  }
  return result;
}

test("Stable and Nightly Linux package file sets never share icons, launchers or executables", () => {
  const stable = JSON.parse(readFileSync(new URL("../apps/desktop/src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  const nightly = mergeConfig(stable, nightlyConfig("0.1.2-nightly.20261026000000", "linux"));
  const identities = JSON.parse(readFileSync(new URL("../brand/settings.json", import.meta.url), "utf8")).desktop_identifiers;
  for (const platform of ["deb", "appimage"]) {
    const sets = [stable, nightly].map((config) => {
      const files = config.bundle.linux[platform].files;
      for (const [dest, source] of Object.entries(files)) {
        assert.ok(dest.includes(config.identifier), dest);
        assert.ok(source.includes(config.identifier), source);
        assert.ok(readFileSync(new URL(`../apps/desktop/src-tauri/${source}`, import.meta.url)).length);
      }
      const binary = config.mainBinaryName ?? "ibis-desktop";
      assert.equal(identities[config === stable ? "stable" : "nightly"], config.identifier);
      const template = readFileSync(new URL(`../apps/desktop/src-tauri/${config.bundle.linux.deb.desktopTemplate}`, import.meta.url), "utf8");
      assert.match(template, new RegExp(`^Icon=${config.identifier.replaceAll(".", "\\.")}$`, "m"));
      // Tauri's freedesktop bundler derives additional PNG paths from the main
      // binary, and the desktop basename from productName (CLI 2.12.3).
      const automatic = config.bundle.icon.filter((source) => source.endsWith(".png")).map((source) => {
        const bytes = readFileSync(new URL(`../apps/desktop/src-tauri/${source}`, import.meta.url));
        const retina = source.includes("@2x") ? "@2" : "";
        return `/usr/share/icons/hicolor/${bytes.readUInt32BE(16)}x${bytes.readUInt32BE(20)}${retina}/apps/${binary}.png`;
      });
      return new Set([...Object.keys(files), ...automatic, `/usr/share/applications/${config.productName}.desktop`, `/usr/bin/${binary}`]);
    });
    assert.deepEqual([...sets[0]].filter((path) => sets[1].has(path)), [], platform);
  }
});

test("previous identity qualification skips old, missing and unknown evidence", () => {
  const workflow = readFileSync(new URL("../.github/workflows/preview.yml", import.meta.url), "utf8");
  const gate = workflow.slice(workflow.indexOf("          previous_identifier=$(cat previous/identifier.txt"), workflow.indexOf("          # Standalone, so an unreadable feed"))
    .split("\n").map((line) => line.replace(/^          /, "")).join("\n");
  assert.ok(gate.includes("case"));
  const dir = mkdtempSync(join(tmpdir(), "ibis-qualify-"));
  try {
    mkdirSync(join(dir, "previous"));
    for (const identity of [undefined, "unavailable", "other", "io.github.ibiscite.ibis.nightly", "com.ibiscite.ibis.nightly"]) {
      if (identity !== undefined) writeFileSync(join(dir, "previous/identifier.txt"), identity);
      const result = spawnSync("bash", ["-e", "-o", "pipefail", "-c", `${gate}\necho qualified`], { cwd: dir, encoding: "utf8", timeout: 30000 });
      assert.equal(result.status, 0, result.stderr);
      assert.equal(result.stdout.includes("qualified"), identity === "com.ibiscite.ibis.nightly", String(identity));
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
  for (const platform of ["darwin", "win32"]) {
    assert.equal(nightlyConfig("0.1.2-nightly.20261026000000", platform).mainBinaryName, undefined);
  }
});

test("previous nightly settings use its own source identity without fallback or execution", () => {
  for (const identifier of ["io.github.ibiscite.ibis.nightly", "com.ibiscite.ibis.nightly"]) {
    const source = `  identifier: "${identifier}",\n`;
    assert.equal(previousNightlyIdentity(source), identifier);
    assert.throws(() => previousNightlyIdentity(source + source));
  }
  assert.throws(() => previousNightlyIdentity('identifier: "other",\n'));
  assert.throws(() => previousNightlyIdentity(""));
});

test("nightly versions sit one patch above the release and sort by commit time", () => {
  assert.equal(nightlyVersion("0.1.0", "20261019223000"), "0.1.1-nightly.20261019223000");
  assert.equal(nightlyVersion("1.9.9", "20991231235959"), "1.9.10-nightly.20991231235959");
  for (const [base, time] of [["0.1", "20261019223000"], ["0.1.0-rc.1", "20261019223000"], ["0.1.0", "202610192230"]]) {
    assert.throws(() => nightlyVersion(base, time));
  }
});

test("key IDs match between a key and the signatures it made", () => {
  assert.equal(keyId(testKey), keyId(signature));
  assert.notEqual(keyId(fixture("other-key.pub")), keyId(signature));
  assert.throws(() => keyId("bm90IGEga2V5"));
});

test("the manifest lists every package by type and refuses a signature from another key", () => {
  const signatures = Object.fromEntries(Object.values(PACKAGES).map((file) => [file, signature]));
  const base = "https://github.com/ibiscite/ibis/releases/download/nightly";
  const feed = manifest({ version: "9.9.9-nightly.209912312359", notes: "n", date: "2026-10-19T22:30:00Z", base, signatures, publicKey: testKey });
  assert.deepEqual(Object.keys(feed.platforms).sort(), [
    "darwin-aarch64-app", "darwin-x86_64-app", "linux-x86_64-appimage", "windows-x86_64-nsis",
  ]);
  assert.equal(feed.platforms["windows-x86_64-nsis"].url, `${base}/Ibis-Windows-setup.exe`);
  assert.equal(feed.platforms["linux-x86_64-appimage"].signature, signature.trim());
  assert.throws(() => manifest({ version: "9.9.9-nightly.209912312359", notes: "", date: "", base, signatures, publicKey: fixture("other-key.pub") }), /not signed/);
  const missing = { ...signatures, "Ibis-Linux.AppImage": undefined };
  assert.throws(() => manifest({ version: "9.9.9-nightly.209912312359", notes: "", date: "", base, signatures: missing, publicKey: testKey }), /no signature/);
});

test("the shipped app carries the Ibis update key", () => {
  const config = JSON.parse(readFileSync(new URL("../apps/desktop/src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  assert.equal(keyId(config.plugins.updater.pubkey), "7c3427f65b29d445");
  assert.equal(config.plugins.updater.requireSignedVersion, true);
});

test("publication refuses missing or replayed signed versions", () => {
  const make = (version, sig) => manifest({
    version, notes: "", date: "", base: "https://example.invalid",
    signatures: Object.fromEntries(Object.values(PACKAGES).map((file) => [file, sig])), publicKey: testKey,
  });
  const version = "9.9.9-nightly.209912312359";
  assert.equal(make(version, signature).version, version);
  for (const sig of [fixture("package.bin.sig"), signature]) {
    assert.throws(() => make("9.9.9-nightly.209912312400", sig), /signed version/);
  }
  // Publication uses the exact build version; only the client normalizes v.
  for (const v of [`v${version}`, "9.9.9", "9.9.9-nightly.209912312359+other"]) {
    assert.throws(() => make(v, signature), /signed version/);
  }
});

test("both publication paths bind the same version they put in the feed", () => {
  const workflow = readFileSync(new URL("../.github/workflows/preview.yml", import.meta.url), "utf8");
  assert.equal((workflow.match(/signer sign --app-version "\$VERSION"/g) ?? []).length, 2);
  assert.match(workflow, /VERSION="\$\{TAG#v\}"/);
  assert.match(workflow, /VERSION="\$\(node scripts\/update-feed.mjs version\)"/);
});

test("Ibis Nightly is a separate app that reads only the nightly feed", () => {
  const config = nightlyConfig("0.1.1-nightly.20261019223000");
  assert.equal(config.identifier, "com.ibiscite.ibis.nightly");
  assert.equal(config.productName, "Ibis Nightly");
  assert.deepEqual(config.plugins.updater.endpoints, ["https://github.com/ibiscite/ibis/releases/download/nightly/latest.json"]);
  assert.ok(!config.bundle.targets.includes("msi") && !config.bundle.targets.includes("rpm"));
  const stable = JSON.parse(readFileSync(new URL("../apps/desktop/src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  assert.equal(config.plugins.updater.pubkey, NIGHTLY_PUBKEY, "Ibis Nightly trusts only the nightly key");
  assert.notEqual(NIGHTLY_PUBKEY, stable.plugins.updater.pubkey, "nightly and stable updates use separate keys");
  assert.notEqual(keyId(NIGHTLY_PUBKEY), keyId(stable.plugins.updater.pubkey));
  assert.equal(feedPublicKey("0.1.1-nightly.20261019223000", stable.plugins.updater.pubkey), NIGHTLY_PUBKEY);
  assert.equal(feedPublicKey("0.2.0", stable.plugins.updater.pubkey), stable.plugins.updater.pubkey);
});

test("Windows startup fixtures use the shipped channel names and identifiers", () => {
  const stable = JSON.parse(readFileSync(new URL("../apps/desktop/src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  const identities = JSON.parse(readFileSync(new URL("../fixtures/startup/windows-identities.json", import.meta.url), "utf8"));
  assert.deepEqual([stable, nightlyConfig("0.1.1-nightly.20261019223000")].map(({ productName, identifier }) => ({ productName, identifier })), identities);
});

test("the shipped channel identities activate the permanent-identity migration", () => {
  const stable = JSON.parse(readFileSync(new URL("../apps/desktop/src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  assert.equal(stable.identifier, "com.ibiscite.ibis");
  assert.equal(nightlyConfig("0.1.1-nightly.20261019223000").identifier, "com.ibiscite.ibis.nightly");
});

test("Windows installers keep each channel's configured publisher and product name", () => {
  const stable = JSON.parse(readFileSync(new URL("../apps/desktop/src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  const overrides = nightlyConfig("0.1.1-nightly.20261019223000");
  const nightly = { ...stable, ...overrides, bundle: { ...stable.bundle, ...overrides.bundle } };
  for (const [config, product] of [[stable, "Ibis"], [nightly, "Ibis Nightly"]]) {
    assert.equal(config.productName, product);
    assert.equal(config.bundle.publisher, "github");
  }
});

test("versions compare as the updater does, so the stable feed never moves back", () => {
  const ordered = ["0.1.0-nightly.20261019223000", "0.1.0-rc.1", "0.1.0-rc.2", "0.1.0-rc.10", "0.1.0", "0.1.1", "0.2.0", "0.10.0", "1.0.0"];
  for (let i = 0; i + 1 < ordered.length; i++) {
    assert.equal(compareVersions(ordered[i + 1], ordered[i]), 1, `${ordered[i + 1]} > ${ordered[i]}`);
    assert.equal(compareVersions(ordered[i], ordered[i + 1]), -1);
  }
  assert.equal(compareVersions("0.1.0-alpha", "0.1.0-alpha.1"), -1);
  assert.equal(compareVersions("0.1.0", "0.1.0"), 0);
  assert.throws(() => compareVersions("v0.1.0", "0.1.0"));
});

test("a feed names the one key that signed it, so the update test can see a key change", () => {
  const feed = (sig) => ({ platforms: { "linux-x86_64": { signature: sig }, "windows-x86_64": { signature: sig } } });
  assert.equal(feedKeyId(feed(signature)), keyId(testKey));
  assert.throws(() => feedKeyId({ platforms: {} }));
  // A synthetic signature that names the other key: keyId reads only its key ID.
  const otherId = Buffer.from(keyId(fixture("other-key.pub")), "hex");
  const line = Buffer.concat([Buffer.from("Ed"), otherId, Buffer.alloc(64)]).toString("base64");
  const other = Buffer.from(`untrusted comment: synthetic\n${line}\n`).toString("base64");
  assert.notEqual(keyId(other), keyId(signature));
  assert.throws(() => feedKeyId({ platforms: { a: { signature }, b: { signature: other } } }));
});
