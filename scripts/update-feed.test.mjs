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

test("keeping the previous nightly never blocks publication on unavailable identity evidence", { skip: process.platform === "win32" }, () => {
  for (const mode of ["published", "current", "source-error", "unknown", "moved", "tag-error", "no-feed"]) {
    const dir = mkdtempSync(join(tmpdir(), "ibis-previous-"));
    try {
      mkdirSync(join(dir, "scripts"));
      for (const name of ["update-feed.mjs", "keep-previous-nightly.sh"]) copyFileSync(new URL(`./${name}`, import.meta.url), join(dir, "scripts", name));
      // Only GitHub is substituted. Execute the nightly job's script and real CLI/source.
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
      const result = spawnSync("scripts/keep-previous-nightly.sh", [], {
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
      // An AppImage's own libraries never leave its image, so they cannot clash.
      const files = Object.fromEntries(Object.entries(config.bundle.linux[platform].files)
        .filter(([dest]) => !(platform === "appimage" && dest.startsWith("/usr/lib/"))));
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

// The update test's "Decide whether to run" script, from the publisher's
// checkout, with only the network (curl), Git (the version) and jq replaced.
function decide(t, { identity, previousVersion = "0.2.4-nightly.20261030000000", published = "0.2.4-nightly.20261031000000", previousKey = signature, publishedKey = signature, feed = true, curl = 0 } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "ibis-decide-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  for (const name of ["scripts", "previous", "bin"]) mkdirSync(join(dir, name));
  copyFileSync(new URL("./update-feed.mjs", import.meta.url), join(dir, "scripts/update-feed.mjs"));
  const node = process.execPath.replaceAll("'", "'\\''");
  writeFileSync(join(dir, "bin/node"), `#!/bin/sh\nif [ "$1" = scripts/update-feed.mjs ] && [ "$2" = version ]; then\n  echo 0.2.4-nightly.20261031000000\nelse\n  exec '${node}' "$@"\nfi\n`, { mode: 0o755 });
  writeFileSync(join(dir, "bin/curl"), `#!/bin/sh\n[ "$CURL_STATUS" = 0 ] || exit "$CURL_STATUS"\n[ "$2" = https://github.com/example/ibis/releases/download/nightly/latest.json ] && [ "$3" = -o ] || exit 2\nprintf '%s' "$PUBLISHED_FEED" > "$4"\n`, { mode: 0o755 });
  writeFileSync(join(dir, "bin/jq"), `#!/bin/sh\n[ "$1" = -r ] && [ "$2" = .version ] && [ $# = 3 ] || exit 2\nexec '${node}' -e 'console.log(JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8")).version)' "$3"\n`, { mode: 0o755 });
  const feedFor = (version, key) => JSON.stringify({ version, platforms: { "linux-x86_64": { signature: key } } });
  if (feed) writeFileSync(join(dir, "previous/latest.json"), feedFor(previousVersion, previousKey));
  writeFileSync(join(dir, "previous/Ibis-Linux.AppImage"), "invented package");
  if (identity !== undefined) writeFileSync(join(dir, "previous/identifier.txt"), identity);
  writeFileSync(join(dir, "output"), "");
  const result = spawnSync(fileURLToPath(new URL("./update-test-decision.sh", import.meta.url)), [], {
    cwd: dir, encoding: "utf8", timeout: 30000,
    env: { ...process.env, [pathKey]: `${join(dir, "bin")}${delimiter}${process.env[pathKey] ?? ""}`, REPO: "example/ibis", PACKAGE: "Ibis-Linux.AppImage", GITHUB_OUTPUT: join(dir, "output"), CURL_STATUS: String(curl), PUBLISHED_FEED: feedFor(published, publishedKey) },
  });
  return { ...result, output: readFileSync(join(dir, "output"), "utf8") };
}

test("the update test fails missing or unknown identity evidence, skips legacy and qualifies current identity", { skip: process.platform === "win32" }, (t) => {
  for (const identity of [undefined, "", "unavailable", "other", "io.github.ibiscite.ibis.nightly", "com.ibiscite.ibis.nightly"]) {
    const result = decide(t, { identity });
    const known = ["io.github.ibiscite.ibis.nightly", "com.ibiscite.ibis.nightly"].includes(identity);
    assert.equal(result.status, known ? 0 : 1, `${String(identity)}: ${result.stdout}${result.stderr}`);
    assert.equal(result.output, `version=0.2.4-nightly.20261031000000\n${identity === "com.ibiscite.ibis.nightly" ? "run=true\n" : ""}`, String(identity));
    if (!known) assert.match(result.stdout, /::error::.*identity/i);
    if (identity === "io.github.ibiscite.ibis.nightly") assert.match(result.stdout, /::notice::Earlier app identities/);
  }
});

test("the update test runs only when the previous nightly can update to the published build", { skip: process.platform === "win32" }, (t) => {
  const identity = "com.ibiscite.ibis.nightly";
  for (const [scenario, options, notice] of [
    ["no previous feed", { feed: false }, /no update feed/],
    ["another nightly key", { publishedKey: fixture("other-key.pub") }, /another key/],
    ["a newer nightly published", { published: "0.2.4-nightly.20261101000000" }, /published nightly is 0\.2\.4-nightly\.20261101000000/],
    ["the same build", { previousVersion: "0.2.4-nightly.20261031000000" }, /same build/],
  ]) {
    const result = decide(t, { identity, ...options });
    assert.equal(result.status, 0, `${scenario}: ${result.stdout}${result.stderr}`);
    assert.equal(result.output, "version=0.2.4-nightly.20261031000000\n", scenario);
    assert.match(result.stdout, notice, scenario);
  }
  const unreadable = decide(t, { identity, curl: 22 });
  assert.equal(unreadable.status, 22, "an unreadable published feed fails instead of skipping");
  assert.doesNotMatch(unreadable.output, /run=/);
});

test("Ibis Nightly keeps the default main binary name outside Linux", () => {
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
