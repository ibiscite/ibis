// What the picker copies and which references it acts on. Pure; no React.
import type { Locator, SearchResult, ZoteroLibrary } from "../../api";
import type { ReferenceForm } from "../../bindings";
import { say } from "../../i18n";
import { outputName, writesEntries, type OutputId } from "../../format/outputs";

/** A prepared reference freezes its search locator independently of later queries. */
export type CitationReference = SearchResult & {
  /** A locator typed in the details carries `input`, and only the engine reads it
   * (a typed label such as "ch. 3" names its type); one captured from search is canonical. */
  readonly locator?: (Readonly<Locator> & { readonly input?: "editor" }) | null;
  readonly form?: ReferenceForm;
  readonly prefix?: string;
  readonly suffix?: string;
};
export type CitationDetailsChange = Pick<CitationReference, "locator" | "form" | "prefix" | "suffix">;

/** What the engine read around a search (`see vale 417`): the locator off its end
 * and the signal off its start, which a reference added from it takes. */
export type Typed = { readonly locator?: Locator | null; readonly prefix?: string | null } | null | undefined;
export function asTyped(result: SearchResult, typed: Typed): CitationReference {
  return { ...result, locator: typed?.locator, ...typed?.prefix ? { prefix: typed.prefix } : {} };
}

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
