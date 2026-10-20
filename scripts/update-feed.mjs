// Update feed for the in-app updater (ADR 0019).
//
//   node scripts/update-feed.mjs version
//     Prints the nightly version for HEAD: one patch above the workspace
//     version, then `-nightly.` and the commit time (UTC, YYYYMMDDHHMMSS), so
//     every build of the same commit agrees and later commits sort higher.
//
//   node scripts/update-feed.mjs config <version>
//     Prints the Tauri configuration that turns a build into Ibis Nightly.
//
//   node scripts/update-feed.mjs manifest <dir> <download-base> <version> <notes>
//     Writes <dir>/latest.json from the signed packages in <dir>, after
//     checking that every signature names the key built into the app.

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));

/** Update entries name their package type, so an app installed from a .deb,
 * .rpm or .msi is never offered another kind of package. */
export const PACKAGES = {
  "darwin-aarch64-app": "Ibis-macOS-Apple-silicon.app.tar.gz",
  "darwin-x86_64-app": "Ibis-macOS-Intel.app.tar.gz",
  "windows-x86_64-nsis": "Ibis-Windows-setup.exe",
  "linux-x86_64-appimage": "Ibis-Linux.AppImage",
};

export function nightlyVersion(base, commitTime) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(base);
  if (!match) throw new Error(`workspace version is not plain X.Y.Z: ${base}`);
  if (!/^\d{14}$/.test(commitTime)) throw new Error(`commit time is not YYYYMMDDHHMMSS: ${commitTime}`);
  return `${match[1]}.${match[2]}.${Number(match[3]) + 1}-nightly.${commitTime}`;
}

/** Ibis Nightly: a separate app with its own settings and update feed. No MSI
 * (Windows Installer versions cannot carry "nightly") and no .rpm (RPM
 * versions cannot contain "-"). */
export function nightlyConfig(version) {
  return {
    productName: "Ibis Nightly",
    version,
    identifier: "io.github.ibiscite.ibis.nightly",
    bundle: { targets: ["app", "dmg", "nsis", "deb", "appimage"] },
    plugins: { updater: { endpoints: ["https://github.com/ibiscite/ibis/releases/download/nightly/latest.json"] } },
  };
}

/** The 8-byte key ID inside a Tauri (base64-wrapped minisign) key or signature. */
export function keyId(wrapped) {
  const text = Buffer.from(wrapped.trim(), "base64").toString("utf8");
  const line = text.split("\n")[1] ?? "";
  const raw = Buffer.from(line, "base64");
  if (raw.length < 10) throw new Error("not a minisign key or signature");
  return raw.subarray(2, 10).toString("hex");
}

export function manifest({ version, notes, date, base, signatures, publicKey }) {
  const expected = keyId(publicKey);
  const platforms = {};
  for (const [target, file] of Object.entries(PACKAGES)) {
    const signature = signatures[file];
    if (!signature) throw new Error(`no signature for ${file}`);
    if (keyId(signature) !== expected) throw new Error(`${file} was not signed with the Ibis update key`);
    platforms[target] = { signature: signature.trim(), url: `${base}/${file}` };
  }
  return { version, notes, pub_date: date, platforms };
}

function workspaceVersion() {
  const toml = readFileSync(join(root, "Cargo.toml"), "utf8");
  const section = toml.split(/^\[workspace\.package\]$/m)[1] ?? "";
  const match = /^version\s*=\s*"([^"]+)"/m.exec(section);
  if (!match) throw new Error("no [workspace.package] version in Cargo.toml");
  return match[1];
}

function main([command, ...args]) {
  if (command === "version") {
    const time = execFileSync("git", ["show", "-s", "--format=%cd", "--date=format-local:%Y%m%d%H%M%S", "HEAD"], {
      cwd: root,
      env: { ...process.env, TZ: "UTC" },
      encoding: "utf8",
    }).trim();
    console.log(nightlyVersion(workspaceVersion(), time));
  } else if (command === "config" && args.length === 1) {
    console.log(JSON.stringify(nightlyConfig(args[0])));
  } else if (command === "manifest" && args.length === 4) {
    const [dir, base, version, notes] = args;
    const config = JSON.parse(readFileSync(join(root, "apps/desktop/src-tauri/tauri.conf.json"), "utf8"));
    const signatures = Object.fromEntries(
      Object.values(PACKAGES).map((file) => [file, readFileSync(join(dir, `${file}.sig`), "utf8")]),
    );
    const feed = manifest({
      version,
      notes,
      date: new Date().toISOString().replace(/\.\d+Z$/, "Z"),
      base,
      signatures,
      publicKey: config.plugins.updater.pubkey,
    });
    writeFileSync(join(dir, "latest.json"), `${JSON.stringify(feed, null, 2)}\n`);
  } else {
    throw new Error("usage: update-feed.mjs version | config <version> | manifest <dir> <download-base> <version> <notes>");
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main(process.argv.slice(2));
