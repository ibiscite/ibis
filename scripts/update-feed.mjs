// Update feed for the in-app updater (ADR 0019).
//
//   node scripts/update-feed.mjs version
//     Prints the nightly version for HEAD: one patch above the workspace
//     version, then `-nightly.` and the commit time (UTC, YYYYMMDDHHMMSS), so
//     every build of the same commit agrees and later commits sort higher.
//
//   node scripts/update-feed.mjs release
//     Prints the workspace version, which a v tag must name.
//
//   node scripts/update-feed.mjs newer <version> <than>
//     Exits 0 if <version> is a higher semantic version than <than>, else 1.
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

/** Semantic version precedence (semver.org §11), as the updater compares. */
export function compareVersions(a, b) {
  const parse = (v) => {
    const m = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/.exec(v);
    if (!m) throw new Error(`not a semantic version: ${v}`);
    return { core: m.slice(1, 4).map(Number), pre: m[4] ? m[4].split(".") : [] };
  };
  const [x, y] = [parse(a), parse(b)];
  for (let i = 0; i < 3; i++) if (x.core[i] !== y.core[i]) return Math.sign(x.core[i] - y.core[i]);
  if (!x.pre.length || !y.pre.length) return Math.sign(y.pre.length - x.pre.length);
  for (let i = 0; i < Math.max(x.pre.length, y.pre.length); i++) {
    const [p, q] = [x.pre[i], y.pre[i]];
    if (p === undefined || q === undefined) return p === undefined ? -1 : 1;
    if (p === q) continue;
    const [pn, qn] = [/^\d+$/.test(p), /^\d+$/.test(q)];
    if (pn && qn) return Math.sign(Number(p) - Number(q));
    if (pn !== qn) return pn ? -1 : 1;
    return p < q ? -1 : 1;
  }
  return 0;
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
  } else if (command === "newer" && args.length === 2) {
    process.exitCode = compareVersions(args[0], args[1]) > 0 ? 0 : 1;
  } else if (command === "release") {
    console.log(workspaceVersion());
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
    throw new Error("usage: update-feed.mjs version | release | newer <version> <than> | config <version> | manifest <dir> <download-base> <version> <notes>");
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main(process.argv.slice(2));
