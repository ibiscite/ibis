import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { PACKAGES, compareVersions, keyId, manifest, nightlyConfig, nightlyVersion } from "./update-feed.mjs";

const fixture = (name) => readFileSync(new URL(`../fixtures/updates/${name}`, import.meta.url), "utf8");
const testKey = fixture("test-key.pub");
const signature = fixture("package-versioned.bin.sig");

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
  assert.equal(config.identifier, "io.github.ibiscite.ibis.nightly");
  assert.equal(config.productName, "Ibis Nightly");
  assert.deepEqual(config.plugins.updater.endpoints, ["https://github.com/ibiscite/ibis/releases/download/nightly/latest.json"]);
  assert.ok(!config.bundle.targets.includes("msi") && !config.bundle.targets.includes("rpm"));
  assert.ok(!("pubkey" in config.plugins.updater), "both apps trust the same built-in key");
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
