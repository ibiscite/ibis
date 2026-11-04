#!/usr/bin/env node
// Runs what Ibis writes through each syntax's own tool, where one can be run.
// Pandoc already reads the Markdown, LaTeX, Typst and Org outputs in the Rust
// tests. The CAYW JSON has no check here: Better BibTeX's clients read it, and no
// tool stands apart from them.
//
//   node scripts/syntax-tools.mjs [--require]
//
// A missing tool is skipped, or fails with --require. IBIS names the binary
// (default target/debug/ibis).
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const references = join(root, "fixtures/syntax-tools/references.bib");
const ibis = resolve(process.env.IBIS ?? join(root, "target/debug/ibis"));

function run(command, args, cwd) {
  return spawnSync(command, args, { cwd, encoding: "utf8", timeout: 300_000, maxBuffer: 64 * 1024 * 1024 });
}

const available = (command, args = ["--version"]) => run(command, args).error?.code !== "ENOENT";

/** What Ibis writes for a citation, which is what a person pastes. */
function cite(format, ...options) {
  const result = run(ibis, ["cite", "taylor2026", "--file", references, "--format", format, ...options]);
  if (result.status !== 0) throw new Error(`ibis cite --format ${format}: ${result.stderr.trim()}`);
  return result.stdout.trim();
}

// Words only, for the checks below; nothing here is shown as HTML. Everything
// between angle brackets goes, and so does a stylesheet's content.
function text(html) {
  let out = "";
  let tag = null;
  let style = false;
  for (const ch of html) {
    if (tag !== null) {
      if (ch !== ">") {
        tag += ch;
        continue;
      }
      const name = tag.trim().toLowerCase();
      if (name.startsWith("style")) style = true;
      else if (name.startsWith("/style")) style = false;
      tag = null;
    } else if (ch === "<") tag = "";
    else if (!style && ch !== ">") out += ch;
  }
  return out.replaceAll("&amp;", "&").replaceAll("\u00a0", " ");
}

function context(work) {
  const citation = cite("context", "--locator", "p. 12", "--prefix", "see & so", "--suffix", "50%, a {b}");
  writeFileSync(join(work, "document.tex"), `\\setupbackend[export=yes]
\\setupbtx[dataset=default]
\\usebtxdataset[references.bib]
\\usebtxdefinitions[apa]
\\starttext
Here ${citation}.

\\placelistofpublications
\\stoptext
`);
  const result = run("context", ["--batchmode", "--nostatistics", "document.tex"], work);
  if (result.status !== 0) throw new Error(`context failed:\n${result.stdout.slice(-1500)}`);
  return text(readFileSync(join(work, "document-export/document-raw.xml"), "utf8"));
}

function myst(work) {
  writeFileSync(join(work, "conf.py"), `extensions = ["myst_parser", "sphinxcontrib.bibtex"]
bibtex_bibfiles = ["references.bib"]
`);
  writeFileSync(join(work, "index.md"), `# Document

Here ${cite("myst")}.

\`\`\`{bibliography}
\`\`\`
`);
  const result = run("sphinx-build", ["-q", "-W", "--keep-going", "-b", "text", ".", "out"], work);
  if (result.status !== 0) throw new Error(`sphinx-build failed:\n${result.stderr.slice(-1500)}`);
  return readFileSync(join(work, "out/index.txt"), "utf8");
}

function asciidoc(work) {
  writeFileSync(join(work, "document.adoc"), `= Document
:bibtex-file: references.bib

Here ${cite("ascii-doc", "--locator", "12")}.

bibliography::[]
`);
  const result = run("asciidoctor", ["-r", "asciidoctor-bibtex", "-o", "document.html", "document.adoc"], work);
  if (result.status !== 0) throw new Error(`asciidoctor failed:\n${result.stderr.slice(-1500)}`);
  return text(readFileSync(join(work, "document.html"), "utf8"));
}

function multimarkdown(work) {
  writeFileSync(join(work, "document.md"), `Here ${cite("multi-markdown", "--locator", "p. 12")}.

[#taylor2026]: Taylor and Dale, Synthetic signals.
`);
  const result = run("multimarkdown", ["document.md"], work);
  if (result.status !== 0) throw new Error(`multimarkdown failed:\n${result.stderr.slice(-1500)}`);
  return result.stdout.replace(/[^]*<body>|<\/body>[^]*/g, "");
}

function jupyter() {
  const script = `import sys
from nbconvert.filters.citation import citation2latex
print(citation2latex(sys.argv[1]))
`;
  const result = run("python3", ["-c", script, cite("jupyter")]);
  if (result.status !== 0) throw new Error(`nbconvert failed:\n${result.stderr.slice(-1500)}`);
  return result.stdout;
}

function jekyll(work) {
  mkdirSync(join(work, "_bibliography"));
  cpSync(join(work, "references.bib"), join(work, "_bibliography/references.bib"));
  writeFileSync(join(work, "_config.yml"), `plugins:
  - jekyll/scholar
scholar:
  style: apa
`);
  writeFileSync(join(work, "index.md"), `---
---
Here ${cite("jekyll-scholar", "--locator", "p. 12")}.
`);
  const result = run("jekyll", ["build", "--quiet"], work);
  if (result.status !== 0) throw new Error(`jekyll failed:\n${result.stderr.slice(-1500)}`);
  return text(readFileSync(join(work, "_site/index.html"), "utf8"));
}

/** Each tool, how to know it is installed, and what its output must say. */
const CHECKS = [
  { name: "ConTeXt", found: () => available("context", ["--help"]), read: context, expect: ["(see & so Taylor & Dale, 2026, p. 12, 50%, a {b})"] },
  { name: "MyST", found: () => available("sphinx-build"), read: myst, expect: ["Taylor", "Synthetic signals"], gone: ["{cite:p}"] },
  { name: "AsciiDoc", found: () => available("asciidoctor"), read: asciidoc, expect: ["Taylor", "12", "Synthetic signals"], gone: ["cite:["] },
  { name: "MultiMarkdown", found: () => available("multimarkdown"), read: multimarkdown, expect: ['href="#taylor2026"', "p. 12"], gone: ["[#taylor2026]", "[p. 12]"] },
  { name: "Jupyter", found: () => run("python3", ["-c", "import nbconvert"]).status === 0, read: jupyter, expect: ["\\cite{taylor2026}"] },
  { name: "Jekyll Scholar", found: () => available("jekyll"), read: jekyll, expect: ["Taylor", "2026", "12"], gone: ["{% cite"] },
];

function main() {
  const required = process.argv.includes("--require");
  if (!existsSync(ibis)) throw new Error(`no ibis at ${ibis}; build it, or set IBIS`);
  let failed = false;
  for (const check of CHECKS) {
    if (!check.found()) {
      console.log(`${check.name}: ${required ? "tool missing" : "skipped, tool not installed"}`);
      failed ||= required;
      continue;
    }
    const work = mkdtempSync(join(tmpdir(), "ibis-syntax-"));
    try {
      cpSync(references, join(work, "references.bib"));
      const output = check.read(work);
      // What the tool must print, and the source it must have replaced.
      const missing = check.expect.filter((word) => !output.includes(word));
      const left = (check.gone ?? []).filter((word) => output.includes(word));
      if (missing.length || left.length) {
        throw new Error(`output lacks ${missing.join(", ")}; still has ${left.join(", ")}:\n${output.slice(0, 1500)}`);
      }
      console.log(`${check.name}: ok`);
    } catch (error) {
      console.log(`${check.name}: failed\n${error.message}`);
      failed = true;
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
  }
  process.exit(failed ? 1 : 0);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
