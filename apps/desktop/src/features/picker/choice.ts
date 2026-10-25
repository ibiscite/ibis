// What the picker copies and which references it acts on. Pure; no React.
import type { CopyFormat, Locator, RecordId, SearchResult, ZoteroLibrary } from "../../api";
import { copyFormatNames } from "../../messages";
import { STYLE_NAMES, type StyleId } from "../../format/styles";

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

/** A prepared reference freezes its search locator independently of later queries. */
export type CitationReference = SearchResult & { readonly locator?: Readonly<Locator> | null };

/** Selection is keyed by record identity, never by list position or citation key. */
export function choiceKey(records: readonly CitationReference[]): string {
  return JSON.stringify(records.map((record) => [record.id.source, record.id.native_key, record.revision, record.locator?.label ?? null, record.locator?.value ?? null]));
}

export function idKey(id: RecordId): string {
  return `${id.source}\u0000${id.native_key}`;
}

export function sameLibrary(a: ZoteroLibrary, b: ZoteroLibrary) {
  return a === b || (typeof a === "object" && typeof b === "object" && a.group === b.group);
}
