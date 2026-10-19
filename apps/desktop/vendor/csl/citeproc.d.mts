// Types for the parts of Zotero's citeproc-js build that Ibis uses.
interface CslSys {
  retrieveLocale(lang: string): string;
  retrieveItem(id: string): unknown;
}

interface CslCite {
  id: string;
  locator?: string;
  label?: string;
}

declare class CslEngine {
  constructor(sys: CslSys, style: string, lang?: string, forceLang?: boolean);
  setOutputFormat(format: "html" | "text" | "rtf"): void;
  updateItems(ids: string[]): void;
  makeBibliography(): [Record<string, unknown>, string[]] | false;
  makeCitationCluster(cites: CslCite[]): string;
}

declare const CSL: { Engine: typeof CslEngine };
export default CSL;
