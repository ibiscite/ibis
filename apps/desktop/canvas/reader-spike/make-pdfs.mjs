// Writes the reader spike's synthetic PDFs (#1252) to a directory, so other
// tools (pdfinfo, pypdf, any reader) can check them independently.
// node apps/desktop/canvas/reader-spike/make-pdfs.mjs OUT_DIR
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { SPECS, brokenPdf, syntheticPdf } from "./synthetic-pdf.ts";

const out = process.argv[2];
if (!out) throw new Error("Usage: make-pdfs.mjs OUT_DIR");
await mkdir(out, { recursive: true });
for (const spec of SPECS) await writeFile(path.join(out, spec.name), syntheticPdf(spec));
await writeFile(path.join(out, "broken.pdf"), brokenPdf());
console.log(`Wrote ${SPECS.length + 1} synthetic PDFs to ${out}`);
