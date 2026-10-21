import type { KeyFormat, LibraryStatus, LoginItemProblem } from "./bindings";

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
  findSources: "Ibis could not look for libraries on this computer. Choose a file instead.",
  openFound: "That file could not be opened from the list. Pick it again from the new list.",
  loginItem: "Ibis could not check whether it starts when you log in. Try again.",
  loginItemChange: "Ibis could not change whether it starts when you log in. Try again.",
} as const;

/** Where people turn login items on and off on each system. */
export function loginItemsPlace(platform: string): string {
  if (/Mac|iPhone|iPad/.test(platform)) return "System Settings > General > Login Items";
  if (/Win/.test(platform)) return "Settings > Apps > Startup";
  return "your desktop's startup applications settings";
}

/** Starting Ibis at login. The menu bar or tray has the same sentences in login_item.rs. */
export const loginItemText = {
  label: (name: string) => `Start ${name} when you log in`,
  title: "Keep the shortcut ready after you restart or log in",
  problem: (problem: LoginItemProblem, place: string, name: string) => ({
    unsupported: `This system cannot start ${name} when you log in. You can add it in ${place}.`,
    needs_approval: `macOS is waiting for your approval. Turn on ${name} in ${place}.`,
    refused: `The system did not let ${name} change this. You can change it in ${place}.`,
    unreadable: `${name} could not check whether it starts when you log in. Look in ${place}.`,
  })[problem],
} as const;

export const setupText = {
  looking: "Looking for Zotero and .bib files…",
  zoteroAccessOff:
    "Zotero is open, but it does not let other apps read it yet. In Zotero, open Settings, then Advanced, and turn on " +
    "“Allow other applications on this computer to communicate with Zotero”.",
  foundFiles: "Found on this computer",
  inFolder: "in",
  openFound: (name: string, folder: string) => (folder ? `Use ${name} in ${folder}` : `Use ${name}`),
  cutShort: "Some folders were too large to search fully. Use Choose library… if your file is not listed.",
  lookAgain: "Look again",
  whereYouWrite: "Where do you write?",
  places: { markdown: "Markdown or Pandoc", latex: "LaTeX", typst: "Typst", word: "Word or Google Docs" },
  styleHint: (style: string) => `Citations copy as ${style}. Pick another style under Copy as.`,
  shortcut: (keys: string) => `Open Ibis from any app with ${keys}.`,
  nothingFound: "No Zotero or .bib files found nearby. Use Choose library… to pick a file.",
  found: (files: number, zotero: boolean) =>
    [zotero ? "Zotero is open, so Use Zotero can read your library." : "",
      files ? `${files} .bib ${files === 1 ? "file" : "files"} found.` : ""].filter(Boolean).join(" "),
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
  snapshotChanged: "The library changed. Search again and reselect the references before copying.",
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
  /** The tile button's tooltip: "Add García 2021 to citation". */
  addNamed: (name: string) => `Add ${name} to citation`,
  removeNamed: (name: string) => `Remove ${name} from citation`,
  bestMatches: "Best matches",
  noMatches: "No matching references",
  noMatchesHint: "Check the spelling, or try fewer words, an author's surname or a year.",
  clearSearch: "Clear search",
  searchAllLibraries: "Search all Zotero libraries",
  typeToFind: "Type to find a reference",
  typeToFindHint: "Search by title, author, year, citation key or DOI.",
  matchedIn: (field: string) => `Matched in ${matchFieldNames[field] ?? field}`,
  more: "More",
  pin: "Pin Ibis",
  pinTitle: "Pin: keep Ibis open beside your document",
  unpinTitle: "Unpin: hide Ibis after copying or clicking elsewhere",
  moreTitle: (keys: string) => `More actions (${keys})`,
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
  resultCount: (count: number) => `${count} ${count === 1 ? "result" : "results"}`,
  selectionCount: (count: number) => `${count} ${count === 1 ? "reference" : "references"} selected for citation`,
  removeReference: (key: string) => `Remove ${key} from the selection`,
  /** When two chosen references from different libraries share a key. */
  removeReferenceIn: (key: string, library: string) => `Remove ${key} (${library}) from the selection`,
  shortcutError: (registered: boolean, shortcut: string, reason: string) => registered
    ? `Your shortcut setting was not used: ${reason}.`
    : `The shortcut ${shortcut} is not available: ${reason}.`,
} as const;

/** The keys overlay and the footer's key hints; see keys.ts for the bindings. */
export const keyText = {
  title: "Keyboard shortcuts",
  close: "Close",
  keyColumn: "Keys",
  actionColumn: "Action",
  regionColumn: "Where",
  or: "or",
  regions: {
    search: "Search box",
    picker: "Anywhere in Ibis",
    menu: "More menu",
    system: "Any app",
  },
  actions: {
    copy: "Copy the highlighted reference, or the ones added to the citation, in the Copy as format",
    add: "Add the highlighted reference to the citation, or remove it",
    move: "Move through the results",
    escape: "Step back: close a menu, clear the search, then hide Ibis",
    more: "Open or close More",
    keys: "Show this list (? works outside the search box)",
    quit: "Quit Ibis",
    menuMove: "Move through the menu",
    menuEnds: "Go to the first or last item",
    menuChoose: "Choose the item",
    menuLeave: "Leave the menu and return to the search",
    open: "Open Ibis",
  },
  allKeys: "All keys",
  footer: "Keys",
  copyAs: (format: string) => `Copy as ${format}`,
  copySeveralAs: (count: number, format: string) => `Copy ${count} as ${format}`,
  addToCitation: "Add to citation",
  removeFromCitation: "Remove from citation",
  closeMenu: "Close menu",
  clearSearch: "Clear search",
  hide: "Hide Ibis",
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
  /** The footer words: the source, then its state. A ticking age is kept
   * out of this text, so the live region never repeats a clock. The source is
   * separate so the footer can isolate its direction and shorten it first. */
  summaryParts: (status: LibraryStatus | null | undefined): [string | null, string] => {
    if (status === undefined) return [null, "Checking library…"];
    if (status === null) return [null, "Library details unavailable"];
    if (status.state === "loading") return [requestedLabel(status), ""];
    if (status.state === "none" || (!status.has_snapshot && !status.name)) return [null, "No library chosen"];
    if (!status.has_snapshot) return [requestedLabel(status), " · not opened"];
    // After a failed switch the previous library is still the one searched: describe it.
    const source = snapshotLabel(status);
    if (status.stale) return [source, " · not updated"];
    if (status.references === 0) return [source, " · no references"];
    return [source, ` · ${count(status.references, "reference")}` +
      (status.warnings > 0 ? ` · ${count(status.warnings, "reading notice")}` : "")];
  },
  summary: (status: LibraryStatus | null | undefined) => {
    const [source, state] = libraryMessages.summaryParts(status);
    return (source ?? "") + state;
  },
  /** The quiet age after the summary, or null when the age does not matter:
   * "updated" only after a real change, "checked" for an old copy. */
  age: (status: LibraryStatus | null | undefined, now: number) => {
    if (status?.state === "loading") return "opening…";
    if (!status?.has_snapshot) return null;
    if (status.state === "reloading") return "reloading…";
    const { updated, checked } = times(status);
    if (status.stale) return checked === null ? null : `checked ${ago(checked, now)}`;
    if (status.error || updated === null || now - updated > recentUpdate) return null;
    return `updated ${ago(updated, now)}`;
  },
  /** Everything known, for the tooltip and the top of More: a short first
   * line (source and count) and a quieter second (libraries, check time, any
   * problem). Names are isolated so right-to-left names keep their order. */
  detail: (status: LibraryStatus | null | undefined, now: number, libraryNames: (status: LibraryStatus) => string): [string, string | null] => {
    const [source, state] = libraryMessages.summaryParts(status);
    const summary = (source === null ? "" : isolate(source)) + state;
    if (!status?.has_snapshot || status.state === "loading") return [summary, null];
    const { checked } = times(status);
    const parts = [
      zoteroSnapshot(status) && !(status.error && !status.stale) ? libraryNames(status) : null,
      checked === null ? null : `checked ${ago(checked, now)}, ${clock(checked, now)}`,
      status.stale ? (zoteroSnapshot(status) ? "Zotero could not be reached" : "the file could not be read")
        : status.error ? `${isolate(status.zotero ? "Zotero" : status.name ?? "the chosen library")} could not be opened` : null,
    ].filter((part): part is string => part !== null);
    const first = status.stale ? `${isolate(snapshotLabel(status))} · ${count(status.references, "reference")}` : summary;
    return [first, parts.length > 0 ? parts.join(" · ") : null];
  },
};

/** Text is English, so numbers and times are too; one place to change with translations. */
export const appLocale = "en";
const minute = 60_000;
/** How long "updated …" stays after the references change. */
export const recentUpdate = 10 * minute;
const numbers = new Intl.NumberFormat(appLocale);
const relative = new Intl.RelativeTimeFormat(appLocale, { numeric: "auto" });

function count(n: number, noun: string) {
  return `${numbers.format(n)} ${n === 1 ? noun : `${noun}s`}`;
}

/** The library asked for, which may not have opened. */
function requestedLabel(status: LibraryStatus) {
  return status.zotero ? "Zotero" : status.name ?? "Library";
}

/** Whether the searched references are Zotero's (older backends: the request). */
function zoteroSnapshot(status: LibraryStatus) {
  return status.snapshot_zotero ?? status.zotero;
}

/** The library actually searched. */
function snapshotLabel(status: LibraryStatus) {
  return zoteroSnapshot(status) ? "Zotero" : status.snapshot_name ?? "Library";
}

/** First-strong isolation for a name inside a sentence (tooltip, accessible name). */
export function isolate(name: string) {
  return `\u2068${name}\u2069`;
}

/** Times from an older backend or fixture may be missing. */
function times(status: LibraryStatus) {
  return { updated: status.updated_at ?? null, checked: status.checked_at ?? null };
}

/** "just now", "5 minutes ago", "yesterday"; a date after a week. Never in the future. */
export function ago(at: number, now: number) {
  const seconds = Math.max(0, (now - at) / 1000);
  if (seconds < 45) return "just now";
  if (seconds < 90 * 60) return relative.format(-Math.max(1, Math.round(seconds / 60)), "minute");
  if (seconds < 36 * 3600) return relative.format(-Math.round(seconds / 3600), "hour");
  if (seconds < 7 * 86400) return relative.format(-Math.round(seconds / 86400), "day");
  return `on ${new Intl.DateTimeFormat(appLocale, { month: "short", day: "numeric" }).format(at)}`;
}

/** Clock time today, otherwise date and time. */
function clock(at: number, now: number) {
  const sameDay = new Date(at).toDateString() === new Date(now).toDateString();
  return new Intl.DateTimeFormat(appLocale, sameDay ? { timeStyle: "short" } : { dateStyle: "medium", timeStyle: "short" }).format(at);
}

const matchFieldNames: Record<string, string> = {
  doi: "the DOI",
  keyword: "the keywords",
  container: "the journal or book",
  abstract: "the abstract",
  identifier: "an ISBN, PubMed ID or arXiv ID",
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

/** Words that tell similar result rows apart (#492). */
export const rowText = {
  /** Follows the first author's name in Compact rows when there are more. */
  etAl: "et al.",
  /** Short word for a CSL item type, shown in Spacious rows; other types show none. */
  itemType: (type: string): string | null => (Object.hasOwn(itemTypeWords, type) ? itemTypeWords[type] ?? null : null),
};

const itemTypeWords: Record<string, string> = {
  "article": "Article",
  "article-journal": "Article",
  "article-magazine": "Article",
  "article-newspaper": "Article",
  "book": "Book",
  "chapter": "Chapter",
  "dataset": "Dataset",
  "entry-dictionary": "Entry",
  "entry-encyclopedia": "Entry",
  "manuscript": "Manuscript",
  "paper-conference": "Paper",
  "patent": "Patent",
  "post": "Post",
  "post-weblog": "Post",
  "report": "Report",
  "review": "Review",
  "review-book": "Review",
  "software": "Software",
  "speech": "Talk",
  "standard": "Standard",
  "thesis": "Thesis",
  "webpage": "Web page",
};

// The notice shown after a clean copy hides the picker (#496).
export const copiedNoticeText = {
  title: "Copied",
  windowsTitle: "Citation copied. Press Ctrl+V to paste.",
  source: "Ibis",
  noticeFailed: "The Copied notice could not be shown.",
};
