// Release notes for a numbered release (#450), used by .github/workflows/preview.yml.
//
//   node scripts/release-notes.mjs <tag> <generated-notes-file>
//     Prints the notes for the release: one plain line, a download table, the
//     unsigned-install note, then the Release Please change list without its
//     greeting, commit hashes or long links. Notes that already have a
//     download table are printed unchanged, so publishing twice is harmless.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const DOWNLOADS = [
  ["macOS, Apple silicon (M1 and later)", "Ibis-macOS-Apple-silicon.dmg"],
  ["macOS, Intel", "Ibis-macOS-Intel.dmg"],
  ["Windows", "Ibis-Windows-setup.exe"],
  ["Linux", "Ibis-Linux.AppImage", " (also `.deb` and `.rpm` below)"],
];

export function changes(generated) {
  const lines = [];
  for (const raw of generated.split("\n")) {
    const line = raw.trimEnd();
    if (/^:robot:|^---$|^## \d/.test(line)) continue;
    const heading = line.match(/^### (.+)/);
    if (heading) {
      lines.push("", `**${heading[1]}**`, "");
      continue;
    }
    const item = line.match(/^[*-] (.+)/);
    if (!item) continue;
    const text = item[1]
      .replace(/ \(\[[0-9a-f]{7,40}\]\([^)]*\)\)/g, "")
      .replace(/, closes \[#\d+\]\([^)]*\)(, \[#\d+\]\([^)]*\))*/g, "")
      .replace(/\[#(\d+)\]\([^)]*\)/g, "#$1")
      .replace(/^\*\*([^*]+):\*\* /, "$1: ");
    lines.push(`- ${text.charAt(0).toUpperCase()}${text.slice(1)}`);
  }
  return lines.join("\n").replace(/\n{3,}/g, "\n\n").trim();
}

export function releaseNotes(tag, generated, repo = "ibiscite/ibis") {
  if (/^## Download$/m.test(generated)) return generated;
  const base = `https://github.com/${repo}/releases/download/${tag}`;
  const rows = DOWNLOADS.map(([system, file, extra = ""]) => `| ${system} | [${file}](${base}/${file})${extra} |`);
  const list = changes(generated);
  return `Ibis ${tag.replace(/^v/, "")}: press a shortcut in any app, find a reference, and copy its citation.

## Download

| System | File |
|---|---|
${rows.join("\n")}

**Preview build. Not signed.** Ibis is not signed by Apple or Microsoft yet. On macOS, if it says the app cannot be checked, right-click Ibis in Applications and choose Open. On Windows, if SmartScreen warns, choose More info, then Run anyway. In-app updates are opt-in and signed, and Ibis checks the signature before installing one.

**Windows installation:** if Microsoft WebView2 Runtime is missing, the installer downloads it once, which needs an internet connection. Ibis works offline after that.

Tell us what works and what doesn't: [report a problem or suggest an idea](https://github.com/${repo}/issues/new/choose).

## Changes
${list ? `\n${list}\n` : "\nSee the commits since the previous release.\n"}
<details>
<summary>Verify a download</summary>

- Checksums: compare \`shasum -a 256 <file>\` (macOS, Linux) or \`Get-FileHash <file>\` (Windows) with the \`SHA256SUMS-*.txt\` files below.
- Provenance: \`gh attestation verify <file> --repo ${repo} --source-ref refs/tags/${tag} --signer-workflow ${repo}/.github/workflows/preview.yml\`

</details>
`;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [tag, file] = process.argv.slice(2);
  if (!/^v\d+\.\d+\.\d+$/.test(tag ?? "") || !file) {
    console.error("usage: node scripts/release-notes.mjs vX.Y.Z <generated-notes-file>");
    process.exit(2);
  }
  process.stdout.write(releaseNotes(tag, readFileSync(file, "utf8"), process.env.GITHUB_REPOSITORY || undefined));
}
