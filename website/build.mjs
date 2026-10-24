import { copyFile, mkdir, mkdtemp, readFile, rename, rm, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { stripTypeScriptTypes } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";

const source = fileURLToPath(new URL(".", import.meta.url));
const root = resolve(source, "..");
const brand = join(root, "brand/exports");
const PUBLIC = ["favicon.svg", "favicon.ico", "site.webmanifest", "ibis-quote-beak.svg",
  "ibis-logo-bird-animated.svg", "ibis-logo-bird-animated-on-dark.svg", "png/og-image.png",
  "png/apple-touch-icon.png", "png/icon-192.png", "png/icon-512.png", "png/icon-maskable-512.png"];

// Build completely before replacing the owned output. The optional source is a test fixture.
export async function build(output = join(source, "dist"), input = source) {
  output = resolve(output); input = resolve(input);
  const inside = (parent, child) => { const p = relative(parent, child); return !p || (p !== ".." && !p.startsWith(".." + sep) && !isAbsolute(p)); };
  if (inside(output, input) || (inside(root, output) && output !== join(source, "dist"))) throw Error("Refusing to replace source files");
  await mkdir(dirname(output), { recursive: true });
  const stage = await mkdtemp(join(dirname(output), ".ibis-build-"));
  try {
    for (const file of ["index.html", "style.css", "site.js", "_headers"]) await copyFile(join(input, file), join(stage, file));
    await copyFile(join(root, "apps/desktop/src/tokens.css"), join(stage, "tokens.css"));
    const keys = stripTypeScriptTypes(await readFile(join(root, "apps/desktop/src/keys.ts"), "utf8"));
    await writeFile(join(stage, "keys.js"), keys);
    for (const file of PUBLIC) {
      const target = join(stage, "assets", file);
      await mkdir(dirname(target), { recursive: true });
      await copyFile(join(brand, file), target);
    }
    await rm(output, { recursive: true, force: true });
    await rename(stage, output);
  } finally { await rm(stage, { recursive: true, force: true }); }
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await build();
  console.log("Built website/dist; publish only its contents.");
}
