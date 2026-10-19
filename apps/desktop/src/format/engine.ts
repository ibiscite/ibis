// Formatted citations with Zotero's build of citeproc-js (ADR 0007). This
// module runs anywhere, including the worker and tests; it never touches the
// network, and styles are data, not code.
import CSL from "../../vendor/csl/citeproc.mjs";
import enUS from "../../vendor/csl/locales/en-US.xml?raw";
import apa from "../../vendor/csl/styles/apa.csl?raw";
import chicago from "../../vendor/csl/styles/chicago-author-date.csl?raw";
import harvard from "../../vendor/csl/styles/harvard-cite-them-right.csl?raw";
import ieee from "../../vendor/csl/styles/ieee.csl?raw";
import mla from "../../vendor/csl/styles/modern-language-association.csl?raw";

export const STYLES = {
  apa: { title: "APA (7th edition)", xml: apa },
  "chicago-author-date": { title: "Chicago (author-date)", xml: chicago },
  "harvard-cite-them-right": { title: "Harvard (Cite Them Right)", xml: harvard },
  ieee: { title: "IEEE", xml: ieee },
  "modern-language-association": { title: "MLA (9th edition)", xml: mla },
} as const;

export type StyleId = keyof typeof STYLES;
export type OutputFormat = "html" | "text" | "rtf";

/** A CSL-JSON item, as `ibis-core` exports it. */
export interface CslItem {
  id: string;
  title?: string;
  [field: string]: unknown;
}

export interface CiteItem {
  id: string;
  locator?: string;
  label?: string;
}

export interface Formatted {
  output: string;
  /** Items shown with a plain fallback because the style could not format them. */
  problems: { id: string; reason: string }[];
}

const FALLBACK_REASON = "this style could not format the reference, so its title is shown instead";

type Engine = InstanceType<typeof CSL.Engine>;

/** One cached engine per style; items are swapped in for each request. */
export class Formatter {
  private engines = new Map<StyleId, Engine>();
  private items = new Map<string, CslItem>();

  private engine(style: StyleId): Engine {
    let engine = this.engines.get(style);
    if (!engine) {
      engine = new CSL.Engine(
        {
          retrieveLocale: () => enUS,
          retrieveItem: (id) => this.items.get(id),
        },
        STYLES[style].xml,
        "en-US",
      );
      this.engines.set(style, engine);
    }
    return engine;
  }

  /** Run with exactly `items` loaded; `items` carry engine keys as ids. */
  private attempt<T>(style: StyleId, items: CslItem[], format: OutputFormat, run: (engine: Engine) => T): T {
    this.items = new Map(items.map((item) => [item.id, item]));
    const engine = this.engine(style);
    try {
      engine.setOutputFormat(format);
      // Forget items from earlier requests: the engine keeps what it fetched
      // by id, and an edited reference must not show its old fields.
      engine.updateItems([]);
      engine.updateItems(items.map((item) => item.id));
      return run(engine);
    } catch (error) {
      // The engine may be left half-updated; start the next request afresh.
      this.engines.delete(style);
      throw error;
    }
  }

  /** The in-text citation for `cites`, such as "(Garcia, 2021, p. 13)". */
  citation(style: StyleId, items: CslItem[], cites: CiteItem[], format: OutputFormat): Formatted {
    const { keyed, keyOf } = engineKeys(items);
    const problems: Formatted["problems"] = [];
    if (cites.length === 0) problems.push({ id: "", reason: "no reference was chosen" });
    const found: CiteItem[] = [];
    const missing: string[] = [];
    for (const cite of cites) {
      const key = keyOf.get(cite.id);
      if (key === undefined) {
        problems.push({ id: cite.id, reason: MISSING_REASON });
        missing.push(escape(cite.id, format));
      } else {
        found.push({ ...cite, id: key });
      }
    }
    // Load every reference, not only the cited ones, and build the
    // bibliography too, so a reference the bibliography leaves out is left out
    // here as well and numbers and year suffixes match it.
    const cluster = (chosen: CiteItem[], loaded: CslItem[]) =>
      chosen.length === 0
        ? ""
        : this.attempt(style, loaded, format, (engine) => {
          engine.makeBibliography();
          return engine.makeCitationCluster(chosen);
        });
    let output: string;
    try {
      output = cluster(found, keyed);
    } catch {
      // Leave out the references this style cannot format; cite the rest together.
      const loadable = keyed.filter((item) => succeeds(() => cluster([{ id: item.id }], [item])));
      let good = found.filter((cite) => loadable.some((item) => item.id === cite.id));
      try {
        output = cluster(good, loadable);
      } catch {
        output = "";
        good = [];
      }
      for (const cite of found.filter((cite) => !good.includes(cite))) {
        const item = keyed.find((candidate) => candidate.id === cite.id);
        problems.push({ id: callerId(items, cite.id), reason: FALLBACK_REASON });
        missing.push(escape(oneLine(item?.title || callerId(items, cite.id)), format));
      }
    }
    const text = [output, ...missing].filter(Boolean).join("; ");
    return { output: wrap(text, format, "citation"), problems };
  }

  /** Bibliography entries for `items`, in the order the style sorts them. */
  bibliography(style: StyleId, items: CslItem[], format: OutputFormat): Formatted {
    const { keyed } = engineKeys(items);
    const run = (chosen: CslItem[]) =>
      chosen.length === 0 ? [] : this.attempt(style, chosen, format, (engine) => entries(engine));
    try {
      return { output: wrap(run(keyed).join("\n"), format, "bibliography"), problems: [] };
    } catch {
      // Format the references that work together, so numbering, sorting and
      // disambiguation stay right, and show the others by title.
      const failing = keyed.filter((item) => !succeeds(() => run([item])));
      let good = keyed.filter((item) => !failing.includes(item));
      let formatted: string[] = [];
      try {
        formatted = run(good);
      } catch {
        // Even the rest together fails: show every reference by title.
        good = [];
      }
      const shown = keyed.filter((item) => !good.includes(item));
      const problems = shown.map((item) => ({ id: callerId(items, item.id), reason: FALLBACK_REASON }));
      const fallback = shown.map((item) => entry(escape(oneLine(item.title || callerId(items, item.id)), format), format));
      const output = [...formatted, ...fallback].join("\n");
      return { output: wrap(output, format, "bibliography"), problems };
    }
  }
}

const MISSING_REASON = "the reference is missing";

function succeeds(run: () => unknown): boolean {
  try {
    run();
    return true;
  } catch {
    return false;
  }
}

/**
 * The engine identifies items by id, but ids from a library are citation
 * keys and can repeat across sources. Give each item its own key for the
 * engine; a citation names the first item with its id.
 */
function engineKeys(items: CslItem[]): { keyed: CslItem[]; keyOf: Map<string, string> } {
  const keyOf = new Map<string, string>();
  const keyed = items.map((item, index) => {
    const key = `ibis-${index}`;
    if (typeof item.id === "string" && !keyOf.has(item.id)) keyOf.set(item.id, key);
    return { ...item, id: key };
  });
  return { keyed, keyOf };
}

function callerId(items: CslItem[], key: string): string {
  const id = items[Number(key.slice("ibis-".length))]?.id;
  return typeof id === "string" ? id : "";
}

function entries(engine: Engine): string[] {
  const bibliography = engine.makeBibliography();
  if (!bibliography) throw new Error("the style has no bibliography");
  // One line per entry: a line break inside a field must not split it.
  return bibliography[1].map(oneLine);
}

function oneLine(text: string): string {
  return text.replace(/\s*\n\s*/g, " ").trim();
}

function entry(text: string, format: OutputFormat): string {
  return format === "html" ? `<div class="csl-entry">${text}</div>` : text;
}

/** RTF output from the engine is a fragment; wrap it as a complete document. */
function wrap(output: string, format: OutputFormat, kind: "citation" | "bibliography"): string {
  if (format !== "rtf") return output;
  const body = kind === "bibliography" ? output.split("\n").join("\\par\n") : output;
  return `{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 Times New Roman;}}\\f0\n${body}\n}`;
}

export function escape(text: string, format: OutputFormat): string {
  switch (format) {
    case "html":
      return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
    case "rtf":
      return [...text]
        .map((ch) => {
          const code = ch.codePointAt(0) ?? 0;
          if (ch === "\\" || ch === "{" || ch === "}") return `\\${ch}`;
          if (code < 0x20 || code === 0x7f) return " ";
          if (code < 0x80) return ch;
          // RTF \u takes a signed 16-bit value; astral characters become surrogate pairs.
          return ch.split("").map((unit) => {
            const value = unit.charCodeAt(0);
            return `\\uc0\\u${value > 0x7fff ? value - 0x10000 : value}{}`;
          }).join("");
        })
        .join("");
    case "text":
      return text;
  }
}
