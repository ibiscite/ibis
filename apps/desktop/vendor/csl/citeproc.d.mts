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

declare const CSL: { Engine: typeof CslEngine };
export default CSL;
