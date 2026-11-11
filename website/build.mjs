import { copyFile, mkdir, mkdtemp, readFile, rename, rm, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { stripTypeScriptTypes } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";

import { content, pages, notFound } from "./content.mjs";

const source = fileURLToPath(new URL(".", import.meta.url));
const root = resolve(source, "..");
const brand = join(root, "brand/exports");
const PUBLIC = ["favicon.svg", "favicon.ico", "site.webmanifest", "ibis-quote-beak.svg",
  "ibis-mark.svg", "ibis-mark-on-dark.svg",
  "ibis-logo.svg", "ibis-logo-on-dark.svg", "ibis-logo-bird-animated.svg", "ibis-logo-bird-animated-on-dark.svg", "png/og-image.png",
  "png/apple-touch-icon.png", "png/icon-192.png", "png/icon-512.png", "png/icon-maskable-512.png"];

// Re-declare the canonical roles at the picture boundary: .site's prose roles
// must not leak into the app. Keep every declaration, condition and selector
// suffix (including contrast and notice-window) in its original cascade order.
export const scopePickerTokens = css => css.replaceAll(":root", ":where([data-app-tokens])");

// Build completely before replacing the owned output. The optional source is a test fixture.
export async function build(output = join(source, "dist"), input = source) {
  output = resolve(output); input = resolve(input);
  const inside = (parent, child) => { const p = relative(parent, child); return !p || (p !== ".." && !p.startsWith(".." + sep) && !isAbsolute(p)); };
  if (inside(output, input) || (inside(root, output) && output !== join(source, "dist"))) throw Error("Refusing to replace source files");
  await mkdir(dirname(output), { recursive: true });
  const stage = await mkdtemp(join(dirname(output), ".ibis-build-"));
  try {
    for (const file of ["style.css", "site.js", "_headers", "demo.js", "demo.css", "demo-model.mjs", "robots.txt", "arrow-up-right.svg"]) await copyFile(join(input, file), join(stage, file));
    const shell = await readFile(join(input, "index.html"), "utf8");
    for (const page of [...pages, notFound]) {
      const body = await readFile(join(input, page === notFound ? "404.html" : `pages/${page.file}.html`), "utf8");
      const target = join(stage, page === notFound ? "404.html" : `${page.path}index.html`);
      await mkdir(dirname(target), { recursive: true });
      await writeFile(target, await content(shell.replace("{{body}}", body), page, input));
    }
    await writeFile(join(stage, "sitemap.xml"), `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${pages.map(page => `  <url><loc>https://ibiscite.org${page.path}</loc></url>`).join("\n")}\n</urlset>\n`);
    await copyFile(join(root, "apps/desktop/src/tokens.css"), join(stage, "tokens.css"));
    await writeFile(join(stage, "picker-tokens.css"), scopePickerTokens(await readFile(join(stage, "tokens.css"), "utf8")));
    for (const name of ["keys", "icon-paths"]) {
      const shared = stripTypeScriptTypes(await readFile(join(root, `apps/desktop/src/${name}.ts`), "utf8"));
      await writeFile(join(stage, `${name}.js`), shared);
    }
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
