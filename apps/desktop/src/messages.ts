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
  zoteroGroups: "Ibis could not list your Zotero groups. Check that Zotero is open, then try again.",
  reload: "Ibis could not reload your library. Choose the file again.",
  hide: "Ibis could not hide this window. Try again.",
  pin: "Ibis could not change the pin. Try again.",
  preferences: "Ibis could not read or save your picker choices. They apply until Ibis restarts.",
  quit: "Ibis could not quit. Try again.",
  pickerEvents: "Ibis could not focus the search box. Restart Ibis to try again.",
  libraryEvents: "Your library may not update automatically. Use Reload, or restart Ibis.",
} as const;

export const densityNames = {
  compact: "Compact",
  comfortable: "Comfortable",
  spacious: "Spacious",
} as const;

export const pickerText = {
  searchLabel: "Search references",
  searchPlaceholder: "Search references…",
  dragSearchBar: "Drag the search bar edge to move the picker",
  searchError: "Ibis could not search your library. Use Reload, then try again.",
  copyError: "Nothing was copied. Try again, or use Reload.",
  copied: "Copied",
  keyFormats: "Citation keys",
  formattedStyles: "Formatted citation",
  preview: "Preview",
  previewPending: "Formatting…",
  previewNotReady: "Nothing was copied yet: the citation is still being formatted. Press Enter again in a moment.",
  previewError: "This citation could not be formatted. Choose another style, or copy citation keys.",
  provisional: "Provisional: the number depends on the other references cited in your document.",
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
  bestMatches: "Best matches",
  noMatches: "No matching references",
  noMatchesHint: "Try fewer words, an author's surname or a year.",
  typeToFind: "Type to find a reference",
  typeToFindHint: "Search by title, author, year, citation key or DOI.",
  matchedIn: (field: string) => `Matched in ${matchFieldNames[field] ?? field}`,
  more: "More",
  pin: "Pin Ibis",
  pinTitle: "Pin: keep Ibis open beside your document",
  unpinTitle: "Unpin: hide Ibis after copying or clicking elsewhere",
  moreTitle: (modifier: string) => `More actions (${modifier}K)`,
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
  density: (name: string) => `Rows: ${name}`,
  densityTitle: "Show less or more of each reference",
  largerText: "Larger text",
  zoteroLibraries: "Zotero libraries",
  myLibrary: "My Library",
  groupLibrary: (id: number) => `Group ${id}`,
  keepOneLibrary: "Keep at least one Zotero library on.",
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

const matchFieldNames: Record<string, string> = {
  doi: "the DOI",
  keyword: "the keywords",
  container: "the journal or book",
  abstract: "the abstract",
};

// One word for the closed format control; the list shows the full label.
export const copyFormatNames: Record<KeyFormat, string> = {
  pandoc: "Pandoc",
  autocite: "\\autocite",
  parencite: "\\parencite",
  textcite: "\\textcite",
  cite: "\\cite",
  typst: "Typst",
  keys: "Keys",
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
