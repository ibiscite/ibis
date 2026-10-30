// The three bundled desktop samples. website-demo.test.mjs pins their equality.
// The website never reads a person's library or clipboard.
export const references = [
  { key: "vale2024lanterns", kind: "article", authors: "Vale", year: 2024, title: "Lantern gardens and the rhythm of imaginary cities", where: "Journal of Invented Landscapes" },
  { key: "fen2023clouds", kind: "book", authors: "Fen", year: 2023, title: "A field guide to clockwork clouds", where: "Imaginary Orchard Press" },
  { key: "reed2025maps", kind: "article", authors: "Reed, Vale", year: 2025, title: "Mapping the quiet islands of an invented sea", where: "Review of Fictional Cartography" },
];
export const formats = { pandoc: "Pandoc", autocite: "\\autocite", parencite: "\\parencite", textcite: "\\textcite", cite: "\\cite", typst: "Typst", keys: "Keys only" };

// This example demonstrates key citations only. It does not duplicate the
// engine's locator parser or formatted-citation policy.
function renderKeys(format, keys) {
  if (!keys.length) return "";
  if (format === "pandoc") return `[${keys.map(key => `@${key}`).join("; ")}]`;
  if (format === "typst") return keys.map(key => `@${key}`).join(" ");
  if (format === "keys") return keys.join(", ");
  if (format === "cite") return `\\cite{${keys.join(",")}}`;
  if (["autocite", "parencite", "textcite"].includes(format))
    return `\\${format}${keys.length > 1 ? "s" : ""}${keys.map(key => `{${key}}`).join("")}`;
  return "";
}
// Notice candidates preserve the copied format and count whole references,
// including the case where only the remaining count fits in two lines.
export function noticeCandidates(format, keys) {
  const candidates = [];
  for (let shown = keys.length; shown >= 0; shown--) {
    const remaining = keys.length - shown;
    const citation = renderKeys(format, keys.slice(0, shown));
    candidates.push(shown ? `${citation}${remaining ? ` +${remaining} more` : ""}` : `+${remaining} more`);
  }
  return candidates;
}
export function createDemo() {
  const state = { query: "lantern", selected: 0, format: "pandoc", added: [], copied: null, pasted: null, open: true, preview: true };
  const found = () => {
    const words = state.query.toLowerCase().split(/\p{White_Space}+/u).filter(Boolean);
    return references.filter(ref => words.every(word => `${ref.authors} ${ref.title} ${ref.year} ${ref.key} ${ref.where}`.toLowerCase().includes(word)));
  };
  const output = () => renderKeys(state.format, state.added.length ? state.added : [found()[state.selected]?.key].filter(Boolean));
  const setQuery = query => { state.query = query; state.selected = 0; };
  return { state, found, output, setQuery,
    add(key) {
      if (!references.some(ref => ref.key === key)) return;
      state.added = state.added.includes(key) ? state.added.filter(value => value !== key) : [...state.added, key];
    },
    copy() { const value = output(); if (!value) return false; state.copied = value; return true; },
    paste() { if (state.copied) state.pasted = state.copied; },
    back() { if (state.preview) state.preview = false; else if (state.query) setQuery(""); else state.open = false; },
  };
}
