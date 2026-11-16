// Usage: node check-pictures.mjs BASELINES ACTUAL DIFFS
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";

const [baseline, actual, diffs] = process.argv.slice(2);
if (!baseline || !actual || !diffs) throw new Error("Usage: check-pictures.mjs BASELINES ACTUAL DIFFS");
const pictures = async (dir) => (await readdir(dir)).filter((name) => name.endsWith(".png")).sort();
const expected = await pictures(baseline);
if (!expected.length) throw new Error("Picture baselines are empty. Run CI with refresh_pictures=true on this branch, then bash scripts/update-picture-baselines.sh RUN_ID; review and commit the PNGs (see docs/testing.md).");
const rendered = await pictures(actual);
if (!rendered.length) throw new Error("No pictures were rendered.");
const failures = [];
await mkdir(diffs, { recursive: true });
for (const name of new Set([...expected, ...rendered])) {
  if (!expected.includes(name)) { failures.push(`${name}: missing baseline`); continue; }
  if (!rendered.includes(name)) { failures.push(`${name}: baseline has no rendered picture`); continue; }
  try {
    let before = PNG.sync.read(await readFile(path.join(baseline, name)));
    let after = PNG.sync.read(await readFile(path.join(actual, name)));
    const resized = before.width !== after.width || before.height !== after.height;
    if (resized) {
      failures.push(`${name}: dimensions ${before.width}x${before.height} → ${after.width}x${after.height}`);
      const width = Math.max(before.width, after.width), height = Math.max(before.height, after.height);
      const pad = (image) => {
        const padded = new PNG({ width, height });
        PNG.bitblt(image, padded, 0, 0, image.width, image.height, 0, 0);
        return padded;
      };
      before = pad(before);
      after = pad(after);
    }
    const diff = new PNG({ width: before.width, height: before.height });
    // Ignore tiny colour noise, but count antialiasing changes: pinned fonts and
    // WebKit should agree. At most 0.01% of a picture may differ.
    const changed = pixelmatch(before.data, after.data, diff.data, before.width, before.height,
      { threshold: 0.1, includeAA: true });
    const ratio = changed / (before.width * before.height);
    if (resized || ratio > 0.0001) {
      if (!resized) failures.push(`${name}: ${changed} pixels (${(ratio * 100).toFixed(4)}%) differ; maximum 0.01%`);
      await writeFile(path.join(diffs, name), PNG.sync.write(diff));
    }
  } catch (error) {
    failures.push(`${name}: ${error.message}`);
  }
}
if (failures.length) throw new Error(failures.join("\n"));
console.log(`Pictures: ${rendered.length} match the visual spec.`);
