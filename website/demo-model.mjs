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
export function createDemo() {
  const state = { query: "okafor", selected: 0, format: "pandoc", added: [], copied: null, pasted: null, open: true, preview: true };
  const found = () => {
    const words = state.query.toLowerCase().split(/\s+/).filter(Boolean);
    return references.filter(ref => words.every(word => `${ref.authors} ${ref.title} ${ref.year} ${ref.key} ${ref.where}`.toLowerCase().includes(word)));
  };
  const output = () => {
    const keys = state.added.length ? state.added : [found()[state.selected]?.key].filter(Boolean);
    if (!keys.length) return "";
    if (state.format === "pandoc") return `[${keys.map(key => `@${key}`).join("; ")}]`;
    if (state.format === "typst") return keys.map(key => `@${key}`).join(" ");
    if (state.format === "keys") return keys.join(", ");
    if (state.format !== "cite" && keys.length > 1) return `\\${state.format}s${keys.map(key => `{${key}}`).join("")}`;
    return `\\${state.format}{${keys.join(",")}}`;
  };
  return { state, found, output,
    add(key) { if (!references.some(ref => ref.key === key)) return; state.added = state.added.includes(key) ? state.added.filter(value => value !== key) : [...state.added, key]; },
    copy() { const value = output(); if (!value) return false; state.copied = value; return true; },
    paste() { state.pasted = state.copied; },
    back() { if (state.preview) state.preview = false; else if (state.query) { state.query = ""; state.selected = 0; } else state.open = false; },
  };
}
