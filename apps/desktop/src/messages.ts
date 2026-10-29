import type { KeyFormat, LibraryStatus, LoginItemProblem, PickerCommand, ShortcutRefusal } from "./bindings";
import type { Platform } from "./platform";

export const citationDetailsText = {
  page: "Page",
  pageExample: "12–15",
  pageHint: "A page or range",
  form: "Citation form",
  forms: [
    { value: "normal", label: "Normal" },
    { value: "narrative", label: "Narrative" },
    { value: "author-omitted", label: "Omit author" },
  ] as const,
  edit: (reference: string) => `Edit ${reference}`,
  details: (reference: string) => `Citation details for ${reference}`,
};

/** Production picker text. Accessible names are stable automation contracts.
 * Native error reasons, notes, settings notices, shortcut errors and copy warnings
 * arrive as English strings from Rust; translating those needs structured codes.
 */
export const operationFailures = {
  rowContext: "The reference menu could not open. Use Enter to copy or Shift+Enter to add.",
  status: "Ibis could not check your library. Choose the file again.",
  format: "Ibis could not read your saved copy format. Check Copy as before copying.",
  shortcut: "Ibis could not check the shortcut. Open Ibis from the menu bar or tray.",
  choose: "The file chooser did not open. Try again.",
  zotero: "Ibis could not start reading Zotero. Try again.",
  samples: "Ibis could not change the sample library. Try again.",
  zoteroGroups: "Ibis could not list your Zotero groups. Check that Zotero is open, then try again.",
  reload: "Ibis could not reload your library. Choose the file again.",
  hide: "Ibis could not hide this window. Try again.",
  pin: "Ibis could not change the pin. Try again.",
  preferences: "Ibis could not read or save your picker choices. They apply until Ibis restarts.",
  quit: "Ibis could not quit. Try again.",
  updateEvents: "Update progress is unavailable.",
  pickerEvents: "Ibis could not focus the search box. Restart Ibis to try again.",
  libraryEvents: "Your library may not update automatically. Use Reload, or restart Ibis.",
  findSources: "Ibis could not look for libraries on this computer. Choose a file instead.",
  openFound: "That file could not be opened from the list. Pick it again from the new list.",
  loginItem: "Ibis could not check whether it starts when you log in. Try again.",
  loginItemChange: "Ibis could not change whether it starts when you log in. Try again.",
  updateChecks: "Ibis could not read its update setting. Try again.",
  updateChecksChange: "Ibis could not change its update setting. Try again.",
  checkForUpdates: "Ibis could not check for updates. Try again.",
  settings: "Settings could not open. Try again.",
  appChoices: "Ibis could not read the formats it remembers for apps, so each app starts with its automatic format.",
  appChoice: "Ibis could not save the format for this app. Try again.",
} as const;

/** Where people turn login items on and off on each system. */
export function loginItemsPlace(platform: Platform): string {
  if (platform === "macos") return "System Settings > General > Login Items";
  if (platform === "windows") return "Settings > Apps > Startup";
  return "your desktop's startup applications settings";
}

/**
 * Starting Ibis at login, in each system's own words (#581): Apple's "Open at
 * Login", Microsoft's "sign in", and GNOME header capitalization for menu
 * items. The menu bar or tray has the same words in login_item.rs.
 */
export const loginItemText = {
  /** The More menu item; the menu bar or tray item reads the same. */
  label: (platform: Platform) => ({ macos: "Open at Login", windows: "Start when you sign in", linux: "Start on Login" })[platform],
  /** The setup screen's check box, a sentence that names the app. */
  sentence: (name: string, platform: Platform) => `Start ${name} when you ${signIn(platform)}`,
  title: (platform: Platform) => `Keep the shortcut ready after you restart or ${signIn(platform)}`,
  problem: (problem: LoginItemProblem, platform: Platform, name: string) => {
    const place = loginItemsPlace(platform);
    const when = signIn(platform);
    return ({
      unsupported: `This system cannot start ${name} when you ${when}. You can add it in ${place}.`,
      needs_approval: `macOS is waiting for your approval. Turn on ${name} in ${place}.`,
      refused: `The system did not let ${name} change this. You can change it in ${place}.`,
      unreadable: `${name} could not check whether it starts when you ${when}. Look in ${place}.`,
    })[problem];
  },
} as const;

function signIn(platform: Platform) {
  return platform === "windows" ? "sign in" : "log in";
}

/** Choice screens [C12]: first run, no library, and a library that cannot be opened. */
export const setupText = {
  sourcesTitle: "Where do your references live?",
  sourcesLine: "Ibis reads your library and never changes it.",
  samples: "Try sample references",
  samplesHelp: "Three invented references, kept separate from your library",
  sampleLabel: "Sample",
  sampleTitle: "Sample references",
  sampleSearchHelp: "Try lantern, clouds or islands.",
  ownLibrary: "Use your own library",
  ownLibraryHelp: "Choose Zotero or a library file instead of samples",
  emptyLine: "Choose a library with references.",
  dropFile: "You can drop a file here too.",
  zotero: "Zotero",
  zoteroHelp: "Reads Zotero 7–10, open or closed; uses a private copy when closed",
  foundOn: (platform: Platform) => `Found on this ${platform === "macos" ? "Mac" : platform === "windows" ? "PC" : "computer"}`,
  file: "BibTeX, BibLaTeX or CSL-JSON file",
  fileHelp: "Choose a .bib, .bibtex, .json or .csljson file",
  findLibraries: "Look for libraries on this computer",
  findHelp: "Zotero and .bib files in your usual folders",
  lookAgain: "Look again",
  looking: "Looking for Zotero and .bib files…",
  zoteroAccessOff:
    "To read your references, in Zotero › Settings › Advanced turn on " +
    "“Allow other applications on this computer to communicate with Zotero”.",
  cutShort: "Some folders were too large to search fully. Choose the file yourself if it is not listed.",
  nothingFound: "No Zotero or .bib files found nearby. Choose the file yourself.",
  found: (files: number, zotero: boolean) =>
    [zotero ? "Zotero is open." : "", files ? `${files} .bib ${files === 1 ? "file" : "files"} found.` : ""].filter(Boolean).join(" "),
  missingTitle: (name: string) => `Can't find ${name}`,
  missingLine: "It may have moved, or its drive is disconnected.",
  find: (name: string) => `Find ${name}`,
  findFileHelp: "Choose where it is now",
  unreadableTitle: (name: string) => `Can't read ${name}`,
  zoteroClosedTitle: "Zotero isn't open",
  zoteroClosedLine: "Open Zotero, then try again.",
  zoteroAccessOffTitle: "Zotero isn't sharing your library",
  otherTitle: (name: string) => `Can't open ${name}`,
  theLibrary: "the library",
  tryAgain: "Try again",
  tryAgainHelp: "Read the library again",
  another: "Use another library",
  anotherHelp: "Zotero or another library file",
  /** A backend reason as a sentence: capitalised, with a full stop. */
  sentence: (reason: string) => {
    const text = reason.charAt(0).toUpperCase() + reason.slice(1);
    return /[.!?]$/.test(text) ? text : `${text}.`;
  },
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
  sharedKey: (key: string) => `More than one reference has the key ${key}, so it can't be cited by key. Give one of them a different key in Zotero or your .bib file.`,
  usingLocator: (label: string, value: string) => `Using ${label} ${value}`,
  searchAsTyped: "Search as typed",
  applyLocator: (key: string) => `Apply locator to ${key}`,
  removeLocator: (label: string, value: string, key: string) => `Remove ${label} ${value} from ${key}`,
  copied: "Copied",
  keyFormats: "Key formats",
  formattedStyles: "Citation styles",
  filterFormats: "Filter formats",
  /** The closed format control when the picker knows the app: "APA · Word". */
  formatForApp: (format: string, app: string) => `${format} · ${app}`,
  forApp: (app: string) => `For ${app}`,
  justOnce: "Just this once",
  automatic: "Automatic",
  globalDefault: "Use as global default",
  forgetApps: "Forget formats for apps",
  forgetAppsTitle: "Every app goes back to its automatic format",
  preview: "Preview",
  previewPending: "Formatting…",
  previewNotReady: "Nothing was copied yet: the citation is still being formatted. Press Enter again in a moment.",
  previewError: "This citation could not be formatted. Choose another style, or copy citation keys.",
  entryOnly: (style: string) => `${style} numbers depend on your whole document, so Ibis copies the reference entry instead.`,
  entryOnlyShort: "Reference entry",
  entryLocator: (style: string) => `A reference entry has no page or other locator, so the ${style} copy leaves it out.`,
  entryEmpty: (style: string, shown: string) => `${style} shows nothing for “${shown}”, so Ibis copies that instead.`,
  entryNothing: (style: string) => `${style} shows nothing for a reference, so the copy leaves it out.`,
  shelfStale: "The library changed since these were chosen.",
  warningCount: (n: number) => (n === 1 ? "1 warning" : `${n} warnings`),
  /** The shelf's last chip, for references beyond the ones it shows. */
  moreChosen: (n: number) => `+${n}`,
  allChosen: (n: number) => `Show all ${n} selected references`,
  fewerChosen: "Show fewer selected references",
  fewerChosenShort: "Fewer",
  dismiss: "Dismiss",
  dismissWarnings: "Dismiss copy warnings",
  copyWarnings: "Copy warnings",
  citationSelection: "Citation selection",
  selectedReferences: "Selected references",
  references: "References",
  searchResults: "Search results",
  results: "Results",
  noDate: "No date",
  addedToCitation: "Added to citation",
  /** The tile button's tooltip: "Add García 2021 to citation". */
  addNamed: (name: string) => `Add ${name} to citation`,
  removeNamed: (name: string) => `Remove ${name} from citation`,
  bestMatches: "Best matches",
  pastedList: "Pasted list",
  copyCitations: (count: number) => `Copy ${count} ${count === 1 ? "citation" : "citations"}`,
  pasteInto: (app: string, output: string) => `Paste into ${app} · ${output}`,
  entryOutput: (style: string, count: number) => count === 1 ? `${style} entry` : `${count} ${style} entries`,
  lastOutcome: (outcome: string) => `Last outcome. ${outcome}`,
  missingCitations: (keys: string[]) => `Not in your library: ${keys.join(", ")}`,
  closestMatches: "Closest matches",
  closestMatch: "Closest match",
  closestMatchWarning: (title: string) => `“${title}” is a closest match: only some search words matched.`,
  noMatches: "No matching references",
  noMatchesHint: "Try a surname, year or fewer words.",
  clearSearch: "Clear search",
  searchAllLibraries: "Search all Zotero libraries",
  typeToFind: "Type to find a reference",
  typeToFindHint: "Search by title, author, year, citation key or DOI.",
  matchedIn: (field: string) => `Matched in ${matchFieldNames[field] ?? field}`,
  more: "More",
  keepOpen: "Keep Window Open",
  keepOpenMenu: "Keep window open",
  stopKeepingOpen: "Stop keeping open",
  showPastePreview: "Show what will be pasted",
  moreTitle: (keys: string) => `More actions (${keys})`,
  chooseLibrary: "Choose library…",
  chooseTitle: "Choose a library",
  useZotero: "Use Zotero",
  useZoteroTitle: "Read your Zotero library, open or closed",
  libraryRegion: "Library",
  libraryStatus: "Library status",
  reload: "Reload",
  reloadTitle: "Reload library",
  copyAs: "Copy as",
  quitLabel: "Quit Ibis",
  settings: "Settings…",
  checkForUpdates: "Check for updates…",
  updateAvailable: "Update available…",
  checkAutomatically: "Check for updates automatically",
  density: (name: string) => `Rows: ${name}`,
  densityTitle: "Show less or more of each reference",
  viewChanges: "View changes",
  largerText: "Larger text",
  searchMark: "Ibis mark in search",
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
  or: "or",
  regions: {
    search: "In the search box",
    picker: "Anywhere in Ibis",
    menu: "In a menu",
    system: "From any app",
  },
  actions: {
    preview: "Show what will be pasted",
    copy: "Copy the highlighted reference, or the ones added to the citation, in the Copy as format (numeric styles such as IEEE copy the reference entry)",
    add: "Add the highlighted reference to the citation, or remove it",
    move: "Move through the results",
    escape: "Step back: close a menu, clear the search, then hide Ibis",
    format: "Choose the output format and where to remember it",
    more: "Open or close More",
    pin: "Keep Window Open, or stop keeping it open",
    keys: "Show this list (? works outside the search box)",
    settings: "Open Settings",
    quit: "Quit Ibis",
    menuMove: "Move through the menu",
    menuEnds: "Go to the first or last item",
    menuChoose: "Choose the item",
    menuLeave: "Leave the menu and return to the search",
    open: "Open Ibis",
  },
  allKeys: "Shortcuts",
  footer: "Keys",
  copy: "Copy",
  copyAs: (format: string) => `Copy as ${format}`,
  copySeveralAs: (count: number, format: string) => `Copy ${count} as ${format}`,
  copyEntries: (count: number, style: string) => (count === 1 ? `Copy ${style} entry` : `Copy ${count} ${style} entries`),
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
  emptyTitle: (name: string | null) => `No references in ${name ?? "this library"}`,
  emptyHint: (zotero = false) => zotero
    ? "Add items in Zotero, then reload."
    : "Choose a file with references.",
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
      status.stale && status.error ? libraryMessages.openError(status.name, status.error, status.zotero)
        : status.stale ? (zoteroSnapshot(status) ? "Zotero could not be reached" : "the file could not be read")
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

/** Short relative times, then a date with the year when different. Never in the future. */
export function ago(at: number, now: number) {
  const seconds = Math.max(0, (now - at) / 1000);
  if (seconds < 45) return "just now";
  if (seconds < 90 * 60) return relative.format(-Math.max(1, Math.round(seconds / 60)), "minute");
  if (seconds < 36 * 3600) return relative.format(-Math.round(seconds / 3600), "hour");
  if (seconds < 7 * 86400) return relative.format(-Math.round(seconds / 86400), "day");
  const year = new Date(at).getFullYear() === new Date(now).getFullYear() ? undefined : "numeric";
  return `on ${new Intl.DateTimeFormat(appLocale, { month: "short", day: "numeric", year }).format(at)}`;
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

// The notice shown after a clean copy hides the picker (#496): pasted into the
// app the picker was opened from (#27), or copied and why it was not pasted.
export const copiedNoticeText = {
  title: "Copied",
  pasted: (app: string | null) => (app ? `Pasted into ${app}` : "Pasted"),
  /** Keyed by the destination's copy reason codes; any other reason says "Copied". */
  reasons: {
    "paste-off": "Copied. Pasting is off",
    "unsupported-system": "Copied. Paste it into your document",
    "no-target": "Copied. No app to paste into",
    "no-permission": "Copied. Ibis needs your OK to paste",
    "terminal": "Copied. Not pasted into a terminal",
    "remote-desktop": "Copied. Not pasted into a remote desktop",
    "virtual-machine": "Copied. Not pasted into a virtual machine",
    "secure-field": "Copied. Not pasted into a password field",
    "secure-input": "Copied. Secure typing is on, possibly in another app. Press ⌘V to paste.",
    "paste-unavailable": "Copied. Press ⌘V to paste.",
    "elevated-app": "Copied. Not pasted into an administrator app",
    "key-release-failed": "Copied. Press and release Ctrl and V",
  } as Record<string, string>,
  windowsTitle: "Citation copied. Press Ctrl+V to paste.",
  source: "Ibis",
  noticeFailed: "The Copied notice could not be shown.",
};

// The format menu: each key format's name and what it pastes.
export const copyFormatMenu: Record<KeyFormat, { label: string; sample: string }> = {
  pandoc: { label: "Pandoc key", sample: "[@key]" },
  autocite: { label: "\\autocite", sample: "\\autocite{key}" },
  parencite: { label: "\\parencite", sample: "\\parencite{key}" },
  textcite: { label: "\\textcite", sample: "\\textcite{key}" },
  cite: { label: "\\cite", sample: "\\cite{key}" },
  typst: { label: "Typst key", sample: "@key" },
  keys: { label: "Keys only", sample: "key" },
};

export const updateText = {
  downloading: "Downloading the update…",
  installing: "Checking and installing the update…",
  failed: "The update could not be installed. Use Check for updates in More to try again later.",
};

// The Settings window (#776, docs/design.md "Settings").
export const settingsText = {
  windowTitle: "Ibis Settings",
  pages: "Settings pages",
  general: "General",
  shortcuts: "Shortcuts",
  about: "About",
  groups: { paste: "Pasting", picker: "Picker", updates: "Updates" },
  paste: {
    label: "Paste into the app you came from",
    description: "Enter pastes the citation where you were typing. Off, Enter only copies it.",
    allowed: (list: string) => `Allowed in Privacy & Security › ${list}`,
    allowedNote: "Ibis presses ⌘V once, only in the app you opened it from.",
    guideTitle: "Allow Ibis to paste for you",
    guideWhy: "To paste, allow Ibis in macOS; it only presses ⌘V in the app you came from and never reads what you type.",
    steps: (name: string, list: string) => [
      "Click Open System Settings.",
      `In Privacy & Security › ${list}, turn on ${name}.`,
      "Come back. Ibis notices on its own.",
    ],
    open: "Open System Settings",
    drag: (name: string, list: string) => `Drag ${name} to ${list}`,
    dragKeyboard: "Press to open System Settings.",
    dragFallback: "If Ibis is not in the list, drag it there.",
    dragFailed: "Couldn't drag this copy of Ibis. Use the + button in System Settings to add it.",
    waiting: "Waiting for your OK…",
    granted: "Ibis can paste now.",
    returnFailed: "Couldn't return to your citation. Open Ibis with your shortcut to continue.",
    stale: (name: string) => `Already on, but still waiting? An update gave ${name} a new identity. Open System Settings again: Ibis clears the old switch so you can turn on the new one.`,
    copyInstead: "Just copy",
  },
  picker: {
    rows: "Rows",
    largerText: "Larger text",
    searchMark: "Ibis mark in search",
    searchMarkDescription: "In place of the magnifier",
    preview: "Show what will be pasted",
    previewDescription: "The line above the footer",
  },
  login: { description: "Keeps the shortcut ready after you restart" },
  shortcut: {
    open: "Open Ibis",
    description: "Click the shortcut, then type a new one. Esc cancels.",
    record: "Type a shortcut…",
    useDefault: "Use Default",
    change: "Change…",
    notAssigned: "Not assigned",
    configureFailed: "The desktop could not open its shortcut settings. Try again.",
    inPicker: "In the picker",
    failed: "Ibis could not change the shortcut. Try again.",
    readFailed: "Ibis could not check the shortcut. Try reopening Settings.",
    refusals: {
      invalid: "Ibis cannot use that key combination. Try another shortcut.",
      "needs-modifier": "Include Control, Alt or Command. Shift alone is not enough.",
      "app-shortcut": "That shortcut is used for app commands. Add another modifier.",
      "alt-gr": "Ctrl+Alt with a typing key is used for AltGr on Windows. Try another shortcut.",
      "in-use": "Another app or the system may be using that shortcut. Try another.",
      unavailable: "Global shortcuts are unavailable on this desktop. Open Ibis from your application launcher.",
      "not-saved": "The shortcut works now, but Ibis could not save it for next time. Try again.",
    } satisfies Record<Extract<ShortcutRefusal, string>, string>,
    commands: {
      format: "Choose Output Format",
      pin: "Keep Window Open",
      keys: "Keyboard Shortcuts",
      more: "More",
      preview: "Show What Will Be Pasted",
      settings: "Settings",
      quit: "Quit Ibis",
    } satisfies Record<PickerCommand, string>,
    pickerConflict: (command: string) => `That shortcut is used for ${command}. Try another shortcut.`,
  },
  updates: {
    automatic: "Check for updates automatically",
    automaticDescription: "Reads a small update file from GitHub a few times a day",
    check: "Check now",
    version: (name: string, version: string) => `${name} ${version}`,
    unavailable: "This build does not update itself.",
  },
  failed: "Ibis could not save that. Try again.",
} as const;

/** Native refusals carry only known codes and command identifiers. */
export function shortcutRefusalText(refusal: unknown): string {
  const text = settingsText.shortcut;
  if (typeof refusal === "string" && Object.hasOwn(text.refusals, refusal)) {
    return text.refusals[refusal as Extract<ShortcutRefusal, string>];
  }
  if (typeof refusal === "object" && refusal !== null && "picker-shortcut" in refusal) {
    const command = refusal["picker-shortcut"];
    if (typeof command === "string" && Object.hasOwn(text.commands, command)) {
      return text.pickerConflict(text.commands[command as PickerCommand]);
    }
  }
  return text.failed;
}
