import type { KeyFormat, LibraryStatus } from "./bindings";

/** Production picker text. Accessible names are stable automation contracts.
 * Native error reasons, notes, settings notices, shortcut errors and copy warnings
 * arrive as English strings from Rust; translating those needs structured codes.
 */
export const operationFailures = {
  status: "Ibis could not check your library. Choose the file again.",
  format: "Ibis could not read your saved copy format. Check Copy as before copying.",
  shortcut: "Ibis could not check the shortcut. Open Ibis from the menu bar or tray.",
  choose: "The file chooser did not open. Try again.",
  zotero: "Ibis could not start reading Zotero. Try again.",
  reload: "Ibis could not reload your library. Choose the file again.",
  hide: "Ibis could not hide this window. Try again.",
  quit: "Ibis could not quit. Try again.",
  pickerEvents: "Ibis could not focus the search box. Restart Ibis to try again.",
  libraryEvents: "Your library may not update automatically. Use Reload, or restart Ibis.",
} as const;

export const pickerText = {
  searchLabel: "Search references",
  searchPlaceholder: "Search references…",
  dragSearchBar: "Drag the search bar edge to move the picker",
  searchError: "Ibis could not search your library. Use Reload, then try again.",
  copyError: "Nothing was copied. Try again, or use Reload.",
  copied: "Copied",
  dismiss: "Dismiss",
  dismissWarnings: "Dismiss copy warnings",
  copyWarnings: "Copy warnings",
  citationSelection: "Citation selection",
  selectedReferences: "Selected references",
  remove: "Remove",
  references: "References",
  searchResults: "Search results",
  results: "Results",
  noDate: "No date",
  addedToCitation: "Added to citation",
  noMatches: "No matching references",
  typeToFind: "Type to find a reference",
  firstRun: "Choose a library to get started",
  firstRunChoose: "Choose library to get started",
  chooseLibrary: "Choose library…",
  chooseTitle: "Choose a library",
  chooseShort: "Choose…",
  useZotero: "Use Zotero",
  useZoteroTitle: "Read your Zotero library while Zotero is open",
  libraryRegion: "Library",
  libraryStatus: "Library status",
  reload: "Reload",
  reloadTitle: "Reload library",
  copyAs: "Copy as",
  quit: "Quit",
  quitLabel: "Quit Ibis",
  keyHints: { copy: " Copy · ", add: " Add · ", hide: " Hide" },
  resultCount: (count: number) => `${count} ${count === 1 ? "result" : "results"}`,
  selectionCount: (count: number) => `${count} ${count === 1 ? "reference" : "references"} selected for citation`,
  removeReference: (key: string) => `Remove ${key} from the selection`,
  shortcutError: (registered: boolean, shortcut: string, reason: string) => registered
    ? `Your shortcut setting was not used: ${reason}.`
    : `The shortcut ${shortcut} is not available: ${reason}.`,
} as const;

export const libraryMessages = {
  opening: (name: string | null) => `Opening ${name ?? "library"}…`,
  reloading: (name: string | null) => `Reloading ${name ?? "library"}…`,
  openError: (name: string | null, reason: string, zotero = false) => {
    const sentence = /[.!?]$/.test(reason) ? reason : `${reason}.`;
    if (zotero) return `${name ?? "Zotero"} could not be read. ${sentence} Open Zotero, then reload.`;
    return `The library could not be opened: ${name ?? "the chosen file"}. ${sentence} Choose it again or reconnect the drive.`;
  },
  empty: (name: string | null, zotero = false) => zotero
    ? `No references found in ${name ?? "your Zotero library"}. Add items in Zotero, then reload.`
    : `No references found in ${name ?? "this library"}. Choose a BibTeX or BibLaTeX export with references, then try again.`,
  summary: (status: LibraryStatus | null | undefined) => {
    if (status === undefined) return "Checking library…";
    if (status === null) return "Library details unavailable";
    if (!status.has_snapshot) return status.name ?? "No library chosen";
    return `${status.snapshot_name ?? "Library"}: ${status.references} ${status.references === 1 ? "reference" : "references"}` +
      (status.warnings > 0 ? `, ${status.warnings} reading ${status.warnings === 1 ? "notice" : "notices"}` : "");
  },
};

// Record requires a label for every Rust citation syntax.
export const copyFormatLabels: Record<KeyFormat, string> = {
  pandoc: "Pandoc / Markdown  [@key]",
  autocite: "BibLaTeX  \\autocite{key}",
  parencite: "BibLaTeX  \\parencite{key}",
  textcite: "BibLaTeX  \\textcite{key}",
  cite: "LaTeX  \\cite{key}",
  typst: "Typst  @key",
  keys: "Keys only",
};
