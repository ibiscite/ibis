import { readFileSync, realpathSync } from "node:fs";
import { resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import { gzipSync } from "node:zlib";

export const PICKER_LIMIT = 400 * 1024;

// Count the picker entry and its static dependency graph, once per JS/CSS file.
// Settings, the receipt window and lazy imports do not load with the picker.
export function pickerBudget(dist, limit = PICKER_LIMIT) {
  const root = realpathSync(dist);
  const manifest = JSON.parse(readFileSync(resolve(root, ".vite/manifest.json"), "utf8"));
  const visited = new Set();
  const files = new Set();
  function visit(key) {
    if (visited.has(key)) return;
    visited.add(key);
    const chunk = manifest[key];
    if (!chunk || typeof chunk.file !== "string") throw new Error("Missing picker chunk");
    files.add(chunk.file);
    for (const file of chunk.css ?? []) files.add(file);
    for (const imported of chunk.imports ?? []) visit(imported);
  }
  if (!manifest["index.html"]?.isEntry) throw new Error("Missing picker entry");
  visit("index.html");
  let bytes = 0;
  for (const file of files) {
    if (typeof file !== "string" || !/\.(?:js|css)$/.test(file)) throw new Error("Invalid picker asset");
    const path = realpathSync(resolve(root, file));
    if (!path.startsWith(root + sep)) throw new Error("Picker asset outside build");
    bytes += gzipSync(readFileSync(path), { level: 9 }).length;
  }
  if (bytes > limit) throw new Error(`Picker assets: ${bytes} gzip bytes exceed ${limit}`);
  return { gzip_bytes: bytes, limit_bytes: limit, files: [...files].sort() };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    console.log(JSON.stringify(pickerBudget(process.argv[2] ?? "apps/desktop/dist")));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
