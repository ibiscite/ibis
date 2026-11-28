// Test-only Node oracle over the exact pinned Zotero processor, styles and adapter.
import fs from "node:fs";
import vm from "node:vm";
import { zstdDecompressSync } from "node:zlib";
import assert from "node:assert/strict";
import CSL from "../vendor/citeproc.mjs";
globalThis.CSL = CSL;
const bundle = fs.readFileSync(new URL("../vendor/csl-bundle.bin", import.meta.url));
const json = (frame) => JSON.parse(zstdDecompressSync(frame));
const blocksStart = 16 + bundle.readUInt32LE(8) + bundle.readUInt32LE(12);
const header = json(bundle.subarray(16, 16 + bundle.readUInt32LE(8)));
const rows = json(bundle.subarray(16 + bundle.readUInt32LE(8), blocksStart));
const unpack = (block, offset, length) => {
  const [at, size] = header.blocks[block];
  return zstdDecompressSync(bundle.subarray(blocksStart + at, blocksStart + at + size)).subarray(offset, offset + length).toString("utf8");
};
const styles = new Map(["apa", "chicago-author-date", "harvard-cite-them-right", "ieee", "modern-language-association", "vancouver"].map((style) => {
  const canonical = style === "vancouver" ? "nlm-citation-sequence" : style;
  const row = rows.find((row) => row[0] === canonical);
  return [style, unpack(row[7], row[8], row[9])];
}));
const english = header.locales.find((locale) => locale[0] === "en-US");
globalThis.__ibisLocaleXml = () => unpack(english[3], english[4], english[5]);
const patchCases = ["numeric", "author-date", "label"].map((format) => [format, `<style xmlns="http://purl.org/net/xbiblio/csl"><info><category citation-format='${format}'/></info><macro name="outside"><text variable="citation-number"/></macro><citation><layout><text variable="citation-number"/></layout></citation><bibliography second-field-align='margin'><sort><key variable="citation-number"/></sort><layout><group><text suffix=')' variable='citation-number' prefix='('/><text variable="title"/><text variable="citation-number"></text></group></layout></bibliography></style>`]);
for (const [style, xml] of patchCases) styles.set(style, xml);
const unobservedGetCite = CSL.getCite;
await import("../src/bridge.js");
// Inspect exactly what the bridge hands the processor, without rendering data.
const Engine = CSL.Engine;
try {
  for (const [style] of patchCases) {
    let tree;
    CSL.Engine = function (_, xml) { tree = typeof xml === "string" ? CSL.parseXml(xml) : xml; throw "captured"; };
    assert.throws(() => __ibisFormat({ style, numeric: style === "numeric", items: [], locale: null, keep_order: false }, styles.get(style)), (error) => error === "captured");
    const bibliography = tree.children.find((node) => node.name === "bibliography");
    assert.equal(bibliography.attrs["second-field-align"], style === "numeric" ? undefined : "margin");
    const group = bibliography.children.find((node) => node.name === "layout").children[0];
    assert.equal(group.children.filter((node) => node.attrs.variable === "citation-number").length, style === "numeric" ? 0 : 2);
    assert.equal(tree.children.find((node) => node.name === "citation").children[0].children[0].attrs.variable, "citation-number");
    assert.equal(tree.children.find((node) => node.name === "macro").children[0].attrs.variable, "citation-number");
    assert.equal(bibliography.children[0].children[0].attrs.variable, "citation-number");
  }
} finally { CSL.Engine = Engine; }
// CSL inline strings use the same documented whitespace contract before either
// processor. Keep this independent regex oracle separate from Rust's traversal.
const requests = JSON.parse(fs.readFileSync(0, "utf8"), (_, value) => typeof value === "string" ? value.normalize("NFC").replace(/[\r\n\u0085\u2028\u2029]+/gu, " ").replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u0084\u0086-\u009f]/gu, " ") : value);
const pristine = structuredClone(requests);
// These oracle fixtures contain decoded text and literal CSL markup. Supply the
// adapter's typed-run interface independently of Rust; the direct processor below
// still receives the original rich strings and verifies the adapter's output.
const richFields = ["title", "title-short", "container-title", "container-title-short", "volume-title", "volume-title-short"];
const markup = /(<\/?(?:i|b|sc|sub|sup)>|<span\s+class="no(?:case|decor)"[^>]*>|<span\s+style="font-variant:\s*small-caps;?"[^>]*>|<\/span>)/g;
for (const request of requests) {
  for (const item of request.items) {
    item["ibis-rich-text"] = Object.fromEntries(richFields.filter((field) => typeof item[field] === "string").map((field) => [field,
      item[field].split(markup).map((part, index) => index % 2 ? { markup: part } : { text: part })
    ]));
  }
}
const updateItems = CSL.Engine.prototype.updateItems;
let registrations = 0;
CSL.Engine.prototype.updateItems = function (ids, ...args) {
  if (ids.length) registrations++;
  return updateItems.call(this, ids, ...args);
};
const numericLabels = new Map([
  ["ieee", '<text variable="citation-number" prefix="[" suffix="]"/>'],
  ["vancouver", '<text suffix="." variable="citation-number"/>'],
]);
const observed = requests.map((request) => {
  const before = registrations;
  const output = JSON.parse(__ibisFormat({ ...request, numeric: numericLabels.has(request.style), sentence_titles: request.style === "apa" }, styles.get(request.style)));
  assert.equal(registrations - before, 1, "one item-registration pass per request");
  return output;
});
CSL.Engine.prototype.updateItems = updateItems;
// Independent transparency check: call the pinned processor without the observer.
CSL.getCite = unobservedGetCite;
// A copied bibliography's layout, as Zotero writes it. RTF comes from Zotero's
// own getBibliographyFormatParameters; HTML transcribes the inline styles of
// Zotero.Cite.makeFormattedBibliography, which needs a browser DOM and is
// checked against Zotero itself in fixtures/zotero/quick-copy. One change is
// Word's: a hanging indent sits on each entry, as RTF's half inch, because Word
// reads no indent from the enclosing block.
const zotero = {};
vm.runInNewContext(fs.readFileSync(new URL("../../ibis-sources/vendor/zotero/cite.js", import.meta.url), "utf8"), { Zotero: zotero });
const rtfSettings = (bibliography) => {
  const style = zotero.Cite.getBibliographyFormatParameters(bibliography);
  return (style.tabStops.length ? "\\tx" + style.tabStops.join(" \\tx") + " " : "")
    + `\\li${style.indent} \\fi${style.firstLineIndent} \\sl${style.lineSpacing} \\slmult1 \\sa${style.entrySpacing} `;
};
const htmlLayout = ([layout], html) => {
  const margins = html.includes('<div class="csl-left-margin">');
  const aligned = layout["second-field-align"];
  const line = Math.max(parseInt(layout.linespacing), 1.35) || 1.35;
  const entry = parseInt(layout.entryspacing);
  const offset = Math.max(1, parseInt(layout.maxoffset) - 2);
  const count = html.split('<div class="csl-entry">').length - 1;
  let index = 0;
  return `<div class="csl-bib-body" style="line-height: ${line}; ">`
    + html.replace(/<div class="csl-entry">((?:(?!<\/div>)[\s\S])*?)<\/div>|<div class="csl-entry">/g, (whole, text) => {
      const after = entry && ++index !== count ? entry : 0;
      // Word for Mac keeps a pasted left margin only on a paragraph.
      if (text !== undefined && layout.hangingindent && !margins && !text.includes("<div")) {
        return `<p class="csl-entry" style="margin: 0 0 ${after}em 0.5in; text-indent: -0.5in; ">${text}</p>`;
      }
      const style = (margins ? "clear: left; " : layout.hangingindent ? "margin-left: 0.5in; text-indent: -0.5in; " : "") + (after ? `margin-bottom: ${after}em;` : "");
      const open = style ? `<div class="csl-entry" style="${style}">` : '<div class="csl-entry">';
      return text === undefined ? open : open + text + "</div>";
    })
      .replaceAll('<div class="csl-left-margin">', `<div class="csl-left-margin" style="float: left; padding-right: 0.5em;${aligned ? `text-align: right; width: ${offset}em;` : ""}">`)
      .replaceAll('<div class="csl-right-inline">', `<div class="csl-right-inline" style="margin: 0 .4em 0 ${aligned ? offset + 0.5 : "0"}em;${layout.hangingindent ? "padding-left: 2em; text-indent:-2em;" : ""}">`)
      .replaceAll('<div class="csl-indent">', '<div class="csl-indent" style="margin: .5em 0 0 2em; padding: 0 0 .2em .5em; border-left: 5px solid #ccc;">')
    + "</div>";
};
const expected = pristine.map((request) => {
  let xml = styles.get(request.style).replace(/disambiguate-add-year-suffix="true"/g, 'disambiguate-add-year-suffix="false"');
  const numberLabel = numericLabels.get(request.style);
  if (numberLabel) xml = xml.replace(numberLabel, "").replace(' second-field-align="flush"', "");
  const items = new Map(request.items.map((item) => [item.id, item]));
  const engine = new CSL.Engine({ retrieveLocale: __ibisLocaleXml, retrieveItem: (id) => items.get(id), uppercase_subtitles: request.style === "apa" }, xml, request.locale ?? undefined, request.locale !== null);
  engine.opt.development_extensions.throw_on_empty = true;
  // Zotero's own settings (Zotero.Style.getCiteProc).
  engine.opt.development_extensions.wrap_url_and_doi = true;
  engine.opt.development_extensions.parse_names = false;
  if (request.keep_order) engine.citation_sort.tokens = [];
  const output = {};
  for (const format of ["text", "html", "rtf"]) {
    engine.setOutputFormat(format);
    engine.updateItems([]);
    engine.updateItems(request.items.map((item) => item.id));
    const bibliography = request.bibliography ? engine.makeBibliography() : null;
    const value = bibliography ? bibliography[1].map((text) => text.replace(/\s*\n\s*/g, " ").trim()).join("\n") : engine.makeCitationCluster(request.cites);
    // Every entry ends its paragraph, the last too, for Word.
    output[format] = format === "rtf" ? `{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 Times New Roman;}}\\f0\n${bibliography ? value.split("\n").map((entry) => `${entry}\\par`).join("\n") : value}\n}` : value;
    if (bibliography) output.layout = bibliography;
  }
  return output;
});
for (let index = 0; index < expected.length; index++) {
  for (const format of ["text", "html", "rtf"]) assert.ok(observed[index][format] === expected[index][format], `observer changed canonical ${format}`);
}
// What is copied: a bibliography with its layout around the processor's output.
process.stdout.write(JSON.stringify(expected.map(({ layout, ...output }) => layout ? {
  ...output,
  html: htmlLayout(layout, output.html),
  rtf: output.rtf.replace("\\f0\n", `\\f0 ${rtfSettings(layout)}\n`),
} : output)));
