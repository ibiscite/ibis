// What the picker copies and which references it acts on. Pure; no React.
import type { Locator, SearchResult, ZoteroLibrary } from "../../api";
import type { ReferenceForm } from "../../bindings";
import { say } from "../../i18n";
import { outputName, writesEntries, type OutputId } from "../../format/outputs";

/** A prepared reference freezes its search locator independently of later queries. */
export type CitationReference = SearchResult & {
  /** Only Page edits carry input; locators captured from search are canonical. */
  readonly locator?: (Readonly<Locator> & { readonly input?: "page-editor" }) | null;
  readonly form?: ReferenceForm;
  readonly prefix?: string;
  readonly suffix?: string;
};
export type CitationDetailsChange = Pick<CitationReference, "locator" | "form" | "prefix" | "suffix">;

/** Selection is keyed by record identity, never by list position or citation key. */
export function choiceKey(records: readonly CitationReference[]): string {
  return JSON.stringify(records.map((record) => [record.id, record.revision, record.cite, record.locator?.label ?? null, record.locator?.value ?? null, record.locator?.input ?? null, record.form ?? "normal", record.prefix ?? "", record.suffix ?? ""]));
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
  if (writesEntries(output)) return say("key-copy-entries", { count: count, style: name });
  return count > 1 ? say("key-copy-several-as", { count: String(count), format: name }) : say("key-copy-as", { format: name });
}
