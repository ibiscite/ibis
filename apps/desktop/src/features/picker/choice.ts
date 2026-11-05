// What the picker copies and which references it acts on. Pure; no React.
import type { Locator, SearchResult, ZoteroLibrary } from "../../api";
import type { ReferenceForm } from "../../bindings";
import { keyText } from "../../messages";
import { outputName, writesEntries, type OutputId } from "../../format/outputs";

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

/** What Enter copies in `output`; numbered styles copy reference entries (#1129). */
export function copyLabel(output: OutputId, count: number): string {
  const name = outputName(output);
  if (writesEntries(output)) return keyText.copyEntries(count, name);
  return count > 1 ? keyText.copySeveralAs(count, name) : keyText.copyAs(name);
}
