// Test-only Node oracle over the exact pinned Zotero processor, styles and adapter.
import fs from "node:fs";
import CSL from "../../../apps/desktop/vendor/csl/citeproc.mjs";
globalThis.CSL = CSL;
const base = new URL("../../../apps/desktop/vendor/csl/", import.meta.url);
globalThis.__ibisStyles = ["apa", "chicago-author-date", "harvard-cite-them-right", "ieee", "modern-language-association"].map((style) => [style, fs.readFileSync(new URL(`styles/${style}.csl`, base), "utf8")]);
globalThis.__ibisLocale = fs.readFileSync(new URL("locales/en-US.xml", base), "utf8");
await import("../src/bridge.js");
const requests = JSON.parse(fs.readFileSync(0, "utf8"));
process.stdout.write(JSON.stringify(requests.map((request) => JSON.parse(__ibisFormat(request)))));
