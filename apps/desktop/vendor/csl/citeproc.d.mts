// Types for the parts of Zotero's citeproc-js build that Ibis uses.
interface CslSys {
  retrieveLocale(lang: string): string;
  retrieveItem(id: string): unknown;
  variableWrapper?(params: { itemData?: { id?: unknown }; system_id?: unknown }, pre: string, value: string, post: string): string;
  wrapCitationEntry?(value: string, id?: string, locator?: string, suffix?: string): string;
}

interface CslCite {
  id: string;
  locator?: string;
  label?: string;
}

declare class CslEngine {
  opt: { development_extensions: { apply_citation_wrapper: boolean } };
  constructor(sys: CslSys, style: string, lang?: string, forceLang?: boolean);
  setOutputFormat(format: "html" | "text" | "rtf"): void;
  updateItems(ids: string[]): void;
  makeBibliography(): [Record<string, unknown>, string[]] | false;
  makeCitationCluster(cites: CslCite[]): string;
}

export type CslName = { family?: unknown } | undefined;
type EtAlSpec = { freeters: number; institutions: number; persons: number[] };
export interface CslNameOutput {
  /** Set by Ibis once its "et al." hooks are installed. */
  ibisOthers?: boolean;
  state: { tmp: { sort_key_flag?: boolean; area?: string; rendered_name?: string[] | false } };
  variables: string[];
  etal_spec: Record<string, EtAlSpec>;
  freeters: Record<string, CslName[]>;
  institutions: Record<string, CslName[]>;
  persons: Record<string, CslName[][]>;
  _imposeNameConstraints(lists: Record<string | number, CslName[]>, count: Record<string | number, number>, key: string | number, pos: number): void;
  renderAllNames(): void;
  _setEtAlParameter(type: "freeters" | "institutions" | "persons", v: string, j?: number): void;
}

declare const CSL: { Engine: typeof CslEngine; NameOutput: { prototype: CslNameOutput } };
export default CSL;
