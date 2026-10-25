// Invented references only. The website never reads a person's library or clipboard.
export const references = [
  { key: "okafor2023", kind: "article", authors: "Okafor, Brandt", year: 2023, title: "Wading birds as early signals of wetland change", where: "Journal of Imagined Ecology" },
  { key: "okafor2021", kind: "article", authors: "Okafor", year: 2021, title: "Estuary feeding patterns of the scarlet ibis", where: "Proceedings of the Example Conference on Coasts" },
  { key: "rivera2025", kind: "article", authors: "Rivera", year: 2025, title: "A place for reading notes", where: "Example Journal" },
  { key: "nakamura2024", kind: "book", authors: "Nakamura", year: 2024, title: "Slow reading in fast institutions", where: "Invented Press" },
  { key: "mensah2022", kind: "article", authors: "Mensah, Ito, Varga", year: 2022, title: "Footnotes and the architecture of trust", where: "Example Review" },
  { key: "lindqvist2019", kind: "book", authors: "Lindqvist", year: 2019, title: "The marginalia problem", where: "Notes and Margins" },
];
export const formats = { pandoc: "Pandoc", autocite: "\\autocite", parencite: "\\parencite", textcite: "\\textcite", cite: "\\cite", typst: "Typst", keys: "Keys only" };
// This offline model mirrors the core parser; production parsing stays in Rust.
const labelAliases = {
  page: ["p", "pp", "pg", "pgs", "page", "pages", "s"], chapter: ["ch", "chap", "chaps", "chapter", "chapters"],
  section: ["sec", "sect", "secs", "section", "sections"], paragraph: ["para", "paras", "paragraph", "paragraphs"],
  figure: ["fig", "figs", "figure", "figures"], line: ["l", "ll", "line", "lines"], note: ["n", "nn", "fn", "note", "notes"],
  volume: ["vol", "vols", "volume", "volumes"], verse: ["v", "vv", "verse", "verses"],
};
function readLabel(text) {
  const sign = text.match(/^(§§?|¶¶?)/u);
  if (sign) return { label: sign[0].startsWith("¶") ? "paragraph" : "section", rest: text.slice(sign[0].length) };
  const word = text.match(/^([\p{L}]+)(.*)$/us);
  if (!word) return null;
  const name = word[1].toLowerCase(), rest = word[2], dotted = rest.startsWith(".");
  if (["para", "n", "s"].includes(name) && !dotted || !dotted && !/^\s/u.test(rest)) return null;
  const label = Object.keys(labelAliases).find(label => labelAliases[label].includes(name));
  return label ? { label, rest: dotted ? rest.slice(1) : rest } : null;
}
function isNumber(text, labelled) {
  if (!labelled) return /^(?:[0-9]{1,3}|[0-9]{5,9})$/.test(text);
  if (/^[0-9]/.test(text)) return /^[0-9]{1,9}(?:[a-z]|ff)?$/i.test(text);
  return text.length > 0 && text.length <= 15 && /^m{0,3}(?:cm|cd|d?c{0,3})(?:xc|xl|l?x{0,3})(?:ix|iv|v?i{0,3})$/i.test(text);
}
const isRange = (text, labelled) => {
  const parts = text.split(/[-–—]/u);
  return parts.length <= 2 && parts.every(part => isNumber(part.trim(), labelled));
};
const isValue = (text, labelled) => text.length > 0 && text.split(/[,&]/u).every(part => isRange(part.trim(), labelled));
function valueStart(text) {
  const floor = Math.max(text.lastIndexOf(","), text.lastIndexOf("&")) + 1;
  const starts = [...text.slice(floor).matchAll(/\S+/gu)].slice(-3).map(match => floor + match.index);
  return starts.find(index => isRange(text.slice(index), true));
}
function continuesBefore(query, start, floor) {
  let before = query.slice(0, start).trimEnd();
  while (/[,&]$/u.test(before)) {
    const rest = before.slice(0, -1).trimEnd(), item = valueStart(rest);
    if (item === undefined) { before = rest; break; }
    if (item < floor) return true;
    before = rest.slice(0, item).trimEnd();
  }
  const word = before.search(/\S+$/u);
  return word >= 0 && word < floor && readLabel(`${before.slice(word)} `) !== null;
}
export function splitLocatorQuery(query) {
  const text = query.trimEnd();
  // Every candidate tail fits in 64 UTF-8 bytes, including its label.
  let floor = Math.max(0, text.length - 64);
  while (new TextEncoder().encode(text.slice(floor)).length > 64) floor++;
  for (let start = floor; start < text.length; start++) {
    if (/[\s,]/u.test(text[start]) || start === 0 || !/[\s,]/u.test(text[start - 1])) continue;
    const before = text.slice(0, start).trimEnd(), search = before.replace(/,+$/u, "").trimEnd(), typed = text.slice(start);
    if (!search) continue;
    const labelled = readLabel(typed), value = labelled ? labelled.rest.trim() : typed;
    if (labelled ? isValue(value, true) : before.endsWith(",") && isValue(value, false)) {
      if (continuesBefore(text, start, floor)) return null;
      return { search, locator: { label: labelled?.label ?? "page", value }, typed };
    }
  }
  return null;
}
const abbreviations = { page: ["p.", "pp."], chapter: ["chap."], section: ["sec."], paragraph: ["para."], figure: ["fig."], line: ["l.", "ll."], note: ["n."], volume: ["vol."], verse: ["v.", "vv."] };
const dashValue = (locator, dash) => locator.value.replace(/[-–—]+/gu, dash);
function locatorText(locator, dash = "-", space = " ") {
  const labels = abbreviations[locator.label], plural = /[-–—,]/u.test(locator.value);
  return `${labels[plural ? labels.length - 1 : 0]}${space}${dashValue(locator, dash)}`;
}
// Parser values contain only numbers, letters, ranges and list separators.
const escapeNote = text => text.replaceAll("&", "\\&");
function renderItems(format, items) {
  const warnings = [], hasLocator = items.some(item => item.locator);
  if (!items.length) return { text: "", warnings };
  let text;
  if (format === "keys") {
    if (hasLocator) warnings.push("bare keys cannot carry page numbers or notes; they were left out");
    text = items.map(item => item.key).join(", ");
  } else if (format === "pandoc") {
    text = `[${items.map(({ key, locator }) => `@${key}${locator ? `, ${escapeNote(locatorText(locator))}` : ""}`).join("; ")}]`;
  } else if (format === "typst") {
    text = items.map(({ key, locator }) => `@${key}${locator ? `[${locatorText(locator, "–")}]` : ""}`).join(" ");
  } else if (format === "cite") {
    if (hasLocator && items.length > 1) warnings.push("plain \\cite holds one note per command; one \\cite per reference was used");
    text = hasLocator ? items.map(({ key, locator }) => `\\cite${locator ? `[${escapeNote(locatorText(locator, "--", "~"))}]` : ""}{${key}}`).join(", ")
      : `\\cite{${items.map(item => item.key).join(",")}}`;
  } else {
    const notes = items.map(({ key, locator }) => `${locator ? `[${escapeNote(locator.label === "page" ? dashValue(locator, "--") : locatorText(locator, "--", "~"))}]` : ""}{${key}}`);
    text = `\\${format}${items.length > 1 ? "s" : ""}${notes.join("")}`;
  }
  return { text, warnings };
}
export function createDemo() {
  const state = { query: "okafor", literalQuery: null, selected: 0, format: "pandoc", added: [], locators: {}, copied: null, pasted: null, open: true, preview: true };
  const interpretation = () => state.literalQuery === state.query ? null : splitLocatorQuery(state.query);
  const found = () => {
    const words = (interpretation()?.search ?? state.query).toLowerCase().split(/\s+/).filter(Boolean);
    return references.filter(ref => words.every(word => `${ref.authors} ${ref.title} ${ref.year} ${ref.key} ${ref.where}`.toLowerCase().includes(word)));
  };
  const selections = () => state.added.length ? state.added.map(key => ({ key, locator: state.locators[key] ?? null }))
    : [found()[state.selected]].filter(Boolean).map(ref => ({ key: ref.key, locator: interpretation()?.locator ?? null }));
  const rendered = () => renderItems(state.format, selections());
  const output = () => rendered().text;
  const setQuery = query => { state.query = query; state.literalQuery = null; state.selected = 0; };
  return { state, found, output, rendered, interpretation, selections, setQuery,
    searchAsTyped() { state.literalQuery = state.query; state.selected = 0; },
    add(key) {
      if (!references.some(ref => ref.key === key)) return;
      if (state.added.includes(key)) { state.added = state.added.filter(value => value !== key); delete state.locators[key]; }
      else { state.added = [...state.added, key]; state.locators[key] = interpretation()?.locator ?? null; }
    },
    applyLocator(key) {
      const locator = interpretation()?.locator;
      if (state.added.includes(key) && found()[state.selected]?.key === key && locator) state.locators[key] = { ...locator };
    },
    removeLocator(key) { if (state.added.includes(key)) state.locators[key] = null; },
    copy() { const value = output(); if (!value) return false; state.copied = value; return true; },
    paste() { state.pasted = state.copied; },
    back() { if (state.preview) state.preview = false; else if (state.query) setQuery(""); else state.open = false; },
  };
}
