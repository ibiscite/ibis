// What the picker copies and which references it acts on. Pure; no React.
import type { CopyFormat, Locator, SearchResult, ZoteroLibrary } from "../../api";
import type { ReferenceForm } from "../../bindings";
import { copyFormatMenu, copyFormatNames, keyText } from "../../messages";
import { copiesEntry, STYLE_NAMES, type StyleId } from "../../format/styles";

/** A citation-key syntax, or a citation style written as `style:<id>`. */
export type Choice = CopyFormat | `style:${StyleId}`;

export function styleOf(choice: Choice): StyleId | null {
  return choice.startsWith("style:") ? (choice.slice("style:".length) as StyleId) : null;
}

/** The one word the closed format control shows. */
export function choiceName(choice: Choice): string {
  const style = styleOf(choice);
  return style ? STYLE_NAMES[style] : copyFormatNames[choice as CopyFormat];
}

/** Name the material that lands, without promising a document-aware style. */
export function outputName(choice: Choice): string {
  const style = styleOf(choice);
  if (!style) return copyFormatMenu[choice as CopyFormat].label;
  return `${STYLE_NAMES[style]} · ${copiesEntry(style) ? "reference entry" : style === "chicago-author-date" ? "author-date text" : "text"}`;
}

/** A prepared reference freezes its search locator independently of later queries. */
export type CitationReference = SearchResult & {
  /** Only Page edits carry input; locators captured from search are canonical. */
  readonly locator?: (Readonly<Locator> & { readonly input?: "page-editor" }) | null;
  readonly form?: ReferenceForm;
};
export type CitationDetailsChange = Pick<CitationReference, "locator" | "form">;

/** Selection is keyed by record identity, never by list position or citation key. */
export function choiceKey(records: readonly CitationReference[]): string {
  return JSON.stringify(records.map((record) => [record.id, record.revision, record.cite, record.locator?.label ?? null, record.locator?.value ?? null, record.locator?.input ?? null, record.form ?? "normal"]));
}

export function idKey(id: unknown): string {
  return JSON.stringify(id);
}

export function sameLibrary(a: ZoteroLibrary, b: ZoteroLibrary) {
  return a === b || (typeof a === "object" && typeof b === "object" && a.group === b.group);
}

/** What Enter copies in `choice`; numeric styles copy reference entries (#1129). */
export function copyLabel(choice: Choice, count: number): string {
  const name = choiceName(choice);
  const style = styleOf(choice);
  if (style && copiesEntry(style)) return keyText.copyEntries(count, name);
  return count > 1 ? keyText.copySeveralAs(count, name) : keyText.copyAs(name);
}
