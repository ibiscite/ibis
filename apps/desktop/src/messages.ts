import type { EntryCopy, LibraryStatus, LoginItemProblem, ProjectKeyState, ProjectStatus, ZoteroPresence } from "./bindings";
import { englishMessages } from "./i18n";
import type { Platform } from "./platform";

/** Every word here comes from the one catalogue, locales/en-US.ftl. */
const say = englishMessages.text;

/** The reader (decision 0038). Page numbers are isolated so they never mirror. */
export const readerText = {
  show: say("reader-show"),
  hide: say("reader-hide"),
  cite: say("reader-cite"),
  pageInView: say("reader-page-in-view"),
  thumbnails: say("reader-thumbnails"),
  pages: say("reader-pages"),
  region: (title: string) => say("reader-region", { title }),
  indicator: (printed: string, index: number, count: number) => `p. ${isolate(printed)} · PDF page ${index} of ${count}`,
  unknown: (index: number, count: number) => say("reader-unknown", { index: String(index), count: String(count) }),
  pdfPage: (index: number) => `PDF ${index + 1}`,
  ask: (index: number) => say("reader-ask", { index: String(index) }),
  useNearby: say("reader-use-nearby"),
};

/** Peek (⌘Y): the highlighted reference's full details. */
export const peekText = {
  label: (title: string) => say("peek-label", { title }),
  container: say("peek-container"),
  year: say("peek-year"),
  type: say("peek-type"),
  key: say("peek-key"),
};

export const citationDetailsText = {
  page: say("citation-details-page"),
  none: say("citation-details-none"),
  /** Page is optional; empty means no page, never the word "None". */
  pagePlaceholder: say("citation-details-page-placeholder"),
  pageHint: say("citation-details-page-hint"),
  form: say("citation-details-form"),
  examplesUnavailable: say("citation-details-examples-unavailable"),
  /** Each form by its one name, from the catalogue. */
  forms: [
    { value: "normal", label: englishMessages.text("form-normal") },
    { value: "narrative", label: englishMessages.text("form-narrative") },
    { value: "year-only", label: englishMessages.text("form-year-only") },
  ] as const,
  edit: (reference: string) => say("citation-details-edit", { reference }),
  details: (reference: string) => say("citation-details-details", { reference }),
};

/** Production picker text. Accessible names are stable automation contracts.
 * Native error reasons, notes, settings notices, shortcut errors and copy warnings
 * arrive as English strings from Rust; translating those needs structured codes.
 */
export const operationFailures = {
  recentHistory: say("operation-failures-recent-history"),
  project: say("operation-failures-project"),
  rowContext: say("operation-failures-row-context"),
  status: say("operation-failures-status"),
  shortcut: say("operation-failures-shortcut"),
  choose: say("operation-failures-choose"),
  dropRefused: say("operation-failures-drop-refused"),
  zotero: say("operation-failures-zotero"),
  samples: say("operation-failures-samples"),
  zoteroGroups: say("operation-failures-zotero-groups"),
  reload: say("operation-failures-reload"),
  hide: say("operation-failures-hide"),
  pin: say("operation-failures-pin"),
  preferences: say("operation-failures-preferences"),
  windowSize: say("operation-failures-window-size"),
  quit: say("operation-failures-quit"),
  updateEvents: say("operation-failures-update-events"),
  pickerEvents: say("operation-failures-picker-events"),
  libraryEvents: say("operation-failures-library-events"),
  findSources: say("operation-failures-find-sources"),
  openFound: say("operation-failures-open-found"),
  settings: say("operation-failures-settings"),
  appChoices: say("operation-failures-app-choices"),
  appRead: say("operation-failures-app-read"),
  appChoice: say("operation-failures-app-choice"),
} as const;

/** Where people turn login items on and off on each system. */
export function loginItemsPlace(platform: Platform): string {
  if (platform === "macos") return "System Settings > General > Login Items";
  if (platform === "windows") return "Settings > Apps > Startup";
  return "your desktop’s startup applications settings";
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
  sourcesTitle: say("setup-sources-title"),
  sourcesLine: say("setup-sources-line"),
  returnHint: (shortcut: string) => say("setup-return-hint", { shortcut }),
  shortcutUnavailable: say("setup-shortcut-unavailable"),
  noShortcut: say("setup-no-shortcut"),
  openWithoutShortcut: say("setup-open-without-shortcut"),
  openingLine: say("setup-opening-line"),
  samples: say("setup-samples"),
  samplesHelp: say("setup-samples-help"),
  sampleLabel: say("setup-sample-label"),
  sampleTitle: say("setup-sample-title"),
  sampleSearchHelp: say("setup-sample-search-help"),
  ownLibrary: say("setup-own-library"),
  ownLibraryHelp: say("setup-own-library-help"),
  emptyLine: say("setup-empty-line"),
  zotero: say("setup-zotero"),
  foundOn: (platform: Platform) => `Found on this ${platform === "macos" ? "Mac" : platform === "windows" ? "PC" : "computer"}`,
  findLibraries: say("setup-find-libraries"),
  foundIn: (folder: string) => say("setup-found-in", { folder }),
  chooseFound: say("setup-choose-found"),
  foundTitle: say("setup-found-title"),
  findHelp: say("setup-find-help"),
  lookAgain: say("setup-look-again"),
  looking: say("setup-looking"),
  zoteroAccessOff:
    "To read your references, in Zotero › Settings › Advanced turn on " +
    "“Allow other applications on this computer to communicate with Zotero”.",
  cutShort: say("setup-cut-short"),
  nothingFound: say("setup-nothing-found"),
  found: (files: number, zotero: ZoteroPresence | null) =>
    [zotero === "available" ? "Zotero is open." : zotero === "closed" ? "Zotero is on this computer." : "",
      files ? `${files} .bib ${files === 1 ? "file" : "files"} found.` : ""].filter(Boolean).join(" "),
  missingTitle: (name: string) => say("setup-missing-title", { name }),
  missingLine: say("setup-missing-line"),
  find: (name: string) => say("setup-find", { name }),
  findFileHelp: say("setup-find-file-help"),
  unreadableTitle: (name: string) => say("setup-unreadable-title", { name }),
  /** Ibis reads Zotero open or closed; this time the closed read could not, so opening it is the fix. */
  zoteroClosedTitle: say("setup-zotero-closed-title"),
  zoteroClosedLine: say("setup-zotero-closed-line"),
  zoteroMissingTitle: say("setup-zotero-missing-title"),
  zoteroMissingLine: say("setup-zotero-missing-line"),
  zoteroAccessOffTitle: say("setup-zotero-access-off-title"),
  zoteroAccessOffWaiting: say("setup-zotero-access-off-waiting"),
  /** The help line under a source row: what opening it does here. */
  openZoteroHelp: say("setup-open-zotero-help"),
  openFileHelp: say("setup-open-file-help"),
  addReferencesHelp: say("setup-add-references-help"),
  zoteroProfilesTitle: say("setup-zotero-profiles-title"),
  zoteroProfilesLine: say("setup-zotero-profiles-line"),
  zoteroProfilesFailed: say("setup-zotero-profiles-failed"),
  otherTitle: (name: string) => say("setup-other-title", { name }),
  theLibrary: say("setup-the-library"),
  tryAgain: say("setup-try-again"),
  tryAgainHelp: say("setup-try-again-help"),
  another: say("setup-another"),
  anotherHelp: say("setup-another-help"),
  /** A backend reason as a sentence: capitalised, with a full stop. */
  sentence: (reason: string) => {
    const text = reason.charAt(0).toUpperCase() + reason.slice(1);
    return /[.!?]$/.test(text) ? text : `${text}.`;
  },
} as const;

/** Named for what Ibis knows: what it copied, never a document's bibliography. */
export const sessionText = {
  menu: say("session-menu"),
  title: say("session-title"),
  about: say("session-about"),
  empty: say("session-empty"),
  close: say("session-close"),
  remove: say("session-remove"),
  removeNamed: (name: string) => `Remove ${isolate(name)}`,
  reset: say("session-reset"),
  undoReset: say("session-undo-reset"),
  undoRemove: (name: string | null) => name ? `Undo Remove ${isolate(name)}` : "Undo Remove",
  absent: (count: number) => count === 1 ? "1 reference isn’t in the open library, so it is left out." : `${count} references aren’t in the open library, so they are left out.`,
  leftOut: (names: string[]) => `Left out because this style can’t write them: ${names.map(isolate).join(", ")}.`,
  full: say("session-full"),
  bibliographyIn: (style: string) => `Bibliography in ${isolate(style)}`,
  copy: say("session-copy"),
  save: say("session-save"),
  copied: say("session-copied"),
  saved: (name: string) => `Saved ${isolate(name)}.`,
  changed: say("session-changed"),
  notRead: say("session-not-read"),
  notFormatted: say("session-not-formatted"),
  notCopied: say("session-not-copied"),
  notSaved: say("session-not-saved"),
} as const;

export const writingProjectText = {
  title: say("writing-project-title"),
  loading: say("writing-project-loading"),
  saving: say("writing-project-saving"),
  menu: say("writing-project-menu"),
  about: say("writing-project-about"),
  sessionOnly: say("writing-project-session-only"),
  close: say("writing-project-close"),
  none: say("writing-project-none"),
  empty: say("writing-project-empty"),
  use: (name: string) => say("writing-project-use", { name }),
  selected: say("writing-project-selected"),
  selectedLabel: say("writing-project-selected-label"),
  format: (name: string) => say("writing-project-format", { name }),
  bibliography: (name: string) => say("writing-project-bibliography", { name }),
  noBibliography: say("writing-project-no-bibliography"),
  chooseBibliography: say("writing-project-choose-bibliography"),
  removeBibliography: say("writing-project-remove-bibliography"),
  create: say("writing-project-create"),
  confirmCreate: say("writing-project-confirm-create"),
  startsWith: (format: string) => say("writing-project-starts-with", { format }),
  name: say("writing-project-name"),
  rename: say("writing-project-rename"),
  saveName: say("writing-project-save-name"),
  remove: say("writing-project-remove"),
  confirmRemove: say("writing-project-confirm-remove"),
  removeQuestion: (name: string) => `Remove ${isolate(name)} from Ibis? Its bibliography file stays where it is.`,
  cancel: say("writing-project-cancel"),
  unsupported: say("writing-project-unsupported"),
  notRead: say("writing-project-not-read"),
  notSaved: say("writing-project-not-saved"),
  bibliographyNotSaved: say("writing-project-bibliography-not-saved"),
  remembered: (name: string) => `Saved for ${isolate(name)}.`,
  pick: say("writing-project-pick"),
} as const;

export const projectCheckText = {
  retry: say("project-check-retry"),
  locate: say("project-check-locate"),
} as const;

export const densityNames = {
  compact: say("density-compact"),
  comfortable: say("density-comfortable"),
} as const;

export const pickerText = {
  projectState: (state: ProjectKeyState, name: string, lines: number[] = []) => {
    const file = isolate(name);
    const text = { in: `In ${file}`, missing: `Missing from ${file}`, "could-not-check": `Couldn’t check ${file}`,
      repeated: "Key appears more than once", "may-name-another": "Key may name another reference" }[state];
    const where = lines.length ? ` (${lines.length === 1 ? "line" : "lines"} ${lines.join(", ")})` : "";
    return `${text}${where}`;
  },
  /** The file the rows are marked against, and what is wrong with it if anything. */
  projectDetail: ({ name, state, reason, damaged_lines: lines }: ProjectStatus) => {
    const file = isolate(name);
    const where = lines.length ? ` Check ${lines.length === 1 ? "line" : "lines"} ${lines.join(", ")}.` : "";
    if (state === "reading") return `Reading ${file}`;
    if (state === "damaged") return `Couldn’t read all of ${file}, so nothing is called missing.${where}`;
    if (state === "failed") return `Couldn’t read ${file}. ${reason ?? ""}${where}`.trim();
    return `Checking against ${file}`;
  },
  chooseProject: say("picker-choose-project"),
  chooseOtherProject: say("picker-choose-other-project"),
  stopProject: (name: string) => `Stop Checking ${isolate(name)}`,
  copyProjectEntry: (name: string) => `Copy Entry for ${isolate(name)}`,
  projectEntry: (outcome: EntryCopy, key: string, name: string) => ({
    copied: `Copied the entry for ${isolate(key)} as your file has it, including any file or note field. Check it, then add it to ${isolate(name)}.`,
    "no-original": "This reference has no original BibTeX entry here. Export it from your reference manager in your project’s dialect, keeping its key.",
    changed: "The library or the bibliography changed. Search again, then try again.",
    repeated: "The library file repeats this key, so Ibis can’t tell which entry to copy.",
    dependency: "This entry relies on a crossref, xdata or @string that can’t come with it. Export it from your reference manager, keeping its key.",
  })[outcome],
  /** An editor asked through the endpoint; it receives exactly what is shown. */
  editorWaiting: (format: string) => `Citing for your editor in ${isolate(format)}. Enter sends what you see; Esc cancels.`,
  openZotero: say("picker-open-zotero"),
  showInZotero: say("picker-show-in-zotero"),
  findSourceFile: (name: string | null) => name ? `Find ${isolate(name)}…` : "Find…",
  sourceChanged: say("picker-source-changed"),
  showSourceFolder: (mac: boolean) => mac ? "Show in Finder" : "Show in Folder",
  openSource: (name: string | null) => `Open ${isolate(name ?? "library")}`,
  sourceOpenFailed: (zotero: boolean) => zotero ? "Zotero could not open. Try again." : "The source file could not open. Try again.",
  searchLabel: say("picker-search-label"),
  searchPlaceholder: say("picker-search-placeholder"),
  dragSearchBar: say("picker-drag-search-bar"),
  searchError: say("picker-search-error"),
  copyError: say("picker-copy-error"),
  snapshotChanged: say("picker-snapshot-changed"),
  sharedKey: (key: string) => say("picker-shared-key", { key }),
  usingLocator: (label: string, value: string) => say("picker-using-locator", { label, value }),
  searchAsTyped: say("picker-search-as-typed"),
  applyLocator: (key: string) => say("picker-apply-locator", { key }),
  copied: say("picker-copied"),
  keyFormats: say("picker-key-formats"),
  formattedStyles: say("picker-formatted-styles"),
  filterFormats: say("picker-filter-formats"),
  /** The closed format control when the format belongs to the app: "APA · Word". */
  formatForApp: (format: string, app: string) => say("picker-format-for-app", { format, app }),
  /** The default needs no suffix: an unmarked format is the default. */
  formatDefault: (format: string) => format,
  formatThisPaste: (format: string) => say("picker-format-this-paste", { format }),
  /** Why the format applies and where a pick goes, at the top of the format menu. */
  formatRemembered: (app: string) => say("picker-format-remembered", { app }),
  formatSuggested: (app: string) => say("picker-format-suggested", { app }),
  formatAppDefault: (app: string) => say("picker-format-app-default", { app }),
  formatNoApp: say("picker-format-no-app"),
  formatHeld: (format: string) => say("picker-format-held", { format }),
  pickReplaces: say("picker-pick-replaces"),
  pickRemembered: say("picker-pick-remembered"),
  pickDefault: say("picker-pick-default"),
  pickThisPaste: say("picker-pick-this-paste"),
  pickToRemember: say("picker-pick-to-remember"),
  pickToSetDefault: say("picker-pick-to-set-default"),
  forApp: (app: string) => say("picker-for-app", { app }),
  /** The format menu's one-line heading: where a pick goes. */
  pickHeadingDefault: say("picker-pick-heading-default"),
  pickHeadingThisPaste: say("picker-pick-heading-this-paste"),
  pasteFromNowOn: (app: string) => say("picker-paste-from-now-on", { app }),
  copyOnly: say("picker-copy-only"),
  copyOnlyHere: (app: string) => say("picker-copy-only-here", { app }),
  preview: say("picker-preview"),
  citationDetails: say("picker-citation-details"),
  edit: say("picker-edit"),
  details: say("picker-details"),
  citationWarnings: say("picker-citation-warnings"),
  fullCitation: say("picker-full-citation"),
  windowReset: say("picker-window-reset"),
  undo: say("picker-undo"),
  notices: say("picker-notices"),
  undoFailed: say("picker-undo-failed"),
  previewPending: say("picker-preview-pending"),
  previewError: say("picker-preview-error"),
  previewRefused: say("picker-preview-refused"),
  previewStyleError: (style: string) => say("picker-preview-style-error", { style }),
  entryOnly: (style: string) => say("picker-entry-only", { style }),
  entryOnlyShort: say("picker-entry-only-short"),
  firstNote: (style: string) => say("picker-first-note", { style }),
  firstNoteShort: say("picker-first-note-short"),
  shelfStale: say("picker-shelf-stale"),
  warningCount: (n: number) => (n === 1 ? "1 warning" : `${n} warnings`),
  allChosen: (n: number) => say("picker-all-chosen", { n: String(n) }),
  dismiss: say("picker-dismiss"),
  dismissWarnings: say("picker-dismiss-warnings"),
  copyWarnings: say("picker-copy-warnings"),
  citationSelection: say("picker-citation-selection"),
  selectedReferences: say("picker-selected-references"),
  filterReferences: say("picker-filter-references"),
  references: say("picker-references"),
  searchResults: say("picker-search-results"),
  results: say("picker-results"),
  noDate: say("picker-no-date"),
  addedToCitation: say("picker-added-to-citation"),
  /** The tile button's tooltip: "Add García 2021 to citation". */
  addNamed: (name: string) => say("picker-add-named", { name }),
  removeNamed: (name: string) => say("picker-remove-named", { name }),
  bestMatches: say("picker-best-matches"),
  pastedList: say("picker-pasted-list"),
  copyCitations: (count: number) => `Copy ${count} ${count === 1 ? "citation" : "citations"}`,
  pasteInto: (app: string, output: string) => say("picker-paste-into", { app, output }),
  entryOutput: (style: string, count: number) => count === 1 ? `${style} entry` : `${count} ${style} entries`,
  lastOutcome: (outcome: string) => say("picker-last-outcome", { outcome }),
  missingCitations: (keys: string[]) => `Not in your library: ${keys.join(", ")}`,
  closestMatches: say("picker-closest-matches"),
  closestMatch: say("picker-closest-match"),
  closestMatchWarning: (title: string) => say("picker-closest-match-warning", { title }),
  noMatches: say("picker-no-matches"),
  noMatchesHint: say("picker-no-matches-hint"),
  filterObstacle: say("picker-filter-obstacle"),
  filterObstacleHint: say("picker-filter-obstacle-hint"),
  removeFilter: say("picker-remove-filter"),
  scopeObstacleHint: say("picker-scope-obstacle-hint"),
  notInLibraries: say("picker-not-in-libraries"),
  notInLibrariesHint: say("picker-not-in-libraries-hint"),
  clearSearch: say("picker-clear-search"),
  searchAllLibraries: say("picker-search-all-libraries"),
  typeToFind: say("picker-type-to-find"),
  typeToFindHint: say("picker-type-to-find-hint"),
  matchedIn: (field: string) => `Matched in ${matchFieldNames[field] ?? field}`,
  more: say("picker-more"),
  changeLastCitation: say("picker-change-last-citation"),
  changeCitation: (format: string) => say("picker-change-citation", { format }),
  keepOpen: say("picker-keep-open"),
  keepOpenMenu: say("picker-keep-open-menu"),
  changeFormat: say("picker-change-format"),
  dragCitation: say("picker-drag-citation"),
  /** ">" in the search: the list shows commands, and Enter runs one. */
  commands: say("picker-commands"),
  noCommands: say("picker-no-commands"),
  runCommand: (title: string) => `Run ${title.replace(/…$/, "")}`,
  moreTitle: (keys: string) => say("picker-more-title", { keys }),
  chooseLibrary: say("picker-choose-library"),
  useZotero: say("picker-use-zotero"),
  libraryRegion: say("picker-library-region"),
  pickerActions: say("picker-picker-actions"),
  libraryStatus: say("picker-library-status"),
  reload: say("picker-reload"),
  reloadLibrary: say("picker-reload-library"),
  chooseZoteroProfile: say("picker-choose-zotero-profile"),
  zoteroProfile: say("picker-zotero-profile"),
  chooseLibraryMenu: say("picker-choose-library-menu"),
  copyAs: say("picker-copy-as"),
  quitLabel: say("picker-quit-label"),
  settings: say("picker-settings"),
  density: (name: string) => say("picker-density", { name }),
  rows: say("picker-rows"),
  densityExamples: { compact: say("picker-density-examples-compact"), comfortable: say("picker-density-examples-comfortable") },
  densityTitle: say("picker-density-title"),
  viewChanges: say("picker-view-changes"),
  largerText: say("picker-larger-text"),
  zoteroLibraries: say("picker-zotero-libraries"),
  searchIn: say("picker-search-in"),
  allLibraries: say("picker-all-libraries"),
  myLibrary: say("picker-my-library"),
  groupLibrary: (id: number) => say("picker-group-library", { id: String(id) }),
  keepOneLibrary: say("picker-keep-one-library"),
  resultCount: (count: number) => `${count} ${count === 1 ? "result" : "results"}`,
  selectionCount: (count: number) => `${count} ${count === 1 ? "reference" : "references"} selected for citation`,
  removeReference: (key: string) => say("picker-remove-reference", { key }),
  /** When two chosen references from different libraries share a key. */
  removeReferenceIn: (key: string, library: string) => say("picker-remove-reference-in", { key, library }),
  shortcutError: (registered: boolean, shortcut: string, reason: string) => {
    const ended = /[.!?]$/.test(reason) ? reason : `${reason}.`;
    return registered ? `Your shortcut setting was not used: ${ended}` : `The shortcut ${shortcut} is not available: ${ended}`;
  },
} as const;

/** The keys overlay and the footer's key hints; see keys.ts for the bindings. */
export const keyText = {
  title: say("key-title"),
  close: say("key-close"),
  regions: {
    search: say("key-regions-search"),
    library: say("key-regions-library"),
    picker: say("key-regions-picker"),
    menu: say("key-regions-menu"),
    system: say("key-regions-system"),
    citation: say("key-regions-citation"),
  },
  actions: {
    source: say("key-actions-source"),
    chooseSource: say("key-actions-choose-source"),
    revealSource: say("key-actions-reveal-source"),
    copy: say("key-actions-copy"),
    add: say("key-actions-add"),
    move: say("key-actions-move"),
    peek: say("key-actions-peek"),
    escape: say("key-actions-escape"),
    format: say("key-actions-format"),
    more: say("key-actions-more"),
    changeLast: say("key-actions-change-last"),
    pin: say("key-actions-pin"),
    keys: say("key-actions-keys"),
    settings: say("key-actions-settings"),
    quit: say("key-actions-quit"),
    undo: say("key-actions-undo"),
    textSize: say("key-actions-text-size"),
    citationMove: say("key-actions-citation-move"),
    citationRemove: say("key-actions-citation-remove"),
    menuMove: say("key-actions-menu-move"),
    menuEnds: say("key-actions-menu-ends"),
    menuChoose: say("key-actions-menu-choose"),
    menuLeave: say("key-actions-menu-leave"),
    open: say("key-actions-open"),
  },
  footer: say("key-footer"),
  copy: say("key-copy"),
  copyAs: (format: string) => say("key-copy-as", { format }),
  copySeveralAs: (count: number, format: string) => say("key-copy-several-as", { count: String(count), format }),
  copyEntries: (count: number, style: string) => (count === 1 ? `Copy ${style} entry` : `Copy ${count} ${style} entries`),
  addToCitation: say("key-add-to-citation"),
  removeFromCitation: say("key-remove-from-citation"),
  closeMenu: say("key-close-menu"),
  clearSearch: say("key-clear-search"),
  hide: say("key-hide"),
} as const;

export const libraryMessages = {
  opening: (name: string | null) => `Opening ${name ?? "library"}…`,
  reloading: (name: string | null) => `Reloading ${name ?? "library"}…`,
  openError: (name: string | null, reason: string, zotero = false, problem: LibraryStatus["problem"] = null) => {
    const sentence = /[.!?]$/.test(reason) ? reason : `${reason}.`;
    if (zotero) {
      // Each fix is named once: sharing is a setting, and a profile is chosen here.
      const fix = problem === "zotero_access_off" ? ` ${setupText.zoteroAccessOff}`
        : problem === "zotero_closed" || problem === null ? " Open Zotero, then reload." : "";
      return `${isolate(name ?? "Zotero")} could not be read. ${sentence}${fix}`;
    }
    return `The library could not be opened: ${isolate(name ?? "the chosen file")}. ${sentence} Choose it again or reconnect the drive.`;
  },
  emptyTitle: (name: string | null) => `No references in ${name ?? "this library"}`,
  liveUpdatesOff: say("library-live-updates-off"),
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
    if (status.stale) return [source, status.problem === "missing" && !status.snapshot_zotero ? " moved" : " not updated"];
    return [source, ` · ${status.references === 0 ? "no references" : count(status.references, "reference")}` +
      readingSummary(status) +
      (status.error ? ` · ${isolate(requestedLabel(status))} ${status.problem === "missing" ? "moved" : "not opened"}` : "")];
  },
  summary: (status: LibraryStatus | null | undefined) => {
    const [source, state] = libraryMessages.summaryParts(status);
    return (source ?? "") + state;
  },
  /** The quiet age after the summary, or null when the age does not matter:
   * "updated" only after a real change, "last read" for an old copy. */
  age: (status: LibraryStatus | null | undefined, now: number) => {
    if (status?.state === "loading") return "opening…";
    if (!status?.has_snapshot) return null;
    if (status.state === "reloading") return "reloading…";
    const { updated, checked } = times(status);
    if (status.stale) return checked === null ? null : shortAge(checked, now);
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
      checked === null ? null : isolate(`${status.stale ? "last read" : "checked"} ${ago(checked, now)}, ${clock(checked, now)}`),
      status.error ? libraryMessages.openError(status.name, status.error, status.zotero, status.problem)
        : status.stale ? (zoteroSnapshot(status) ? "Zotero could not be reached" : "the file could not be read")
        : null,
      readingDetail(status),
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

function count(n: number, noun: string, plural = `${noun}s`) {
  return `${numbers.format(n)} ${n === 1 ? noun : plural}`;
}

/** Entries the engine left out of the library (decision 0006). */
const skips = (code: string) => code === "duplicate-key" || code === "syntax";
type Reading = Pick<LibraryStatus, "warnings" | "reading">;
const reading = (status: Reading) => status.reading ?? [];

/** The footer's words for what reading found: entries skipped, then those to
 * check. More problems than the engine details is said as one honest count. */
function readingSummary(status: Reading) {
  if (status.warnings === 0) return "";
  const notices = reading(status);
  if (notices.length < status.warnings) return ` · ${say("library-entries-to-check", { count: status.warnings })}`;
  const skipped = notices.filter(({ code }) => skips(code)).length, check = status.warnings - skipped;
  return (skipped > 0 ? ` · ${count(skipped, "entry", "entries")} skipped` : "") + (check > 0 ? ` · ${numbers.format(check)} to check` : "");
}

/** "line 3", "lines 3 and 9", "lines 3, 9, 12 and 40", then "and others". */
function lines(found: readonly (number | null)[]) {
  const known = found.filter((line): line is number => line !== null).map((line) => numbers.format(line));
  if (known.length === 0) return null;
  if (known.length === 1) return `line ${known[0]}`;
  const shown = known.slice(0, 4);
  return `lines ${shown.slice(0, -1).join(", ")} and ${known.length > 4 ? `${shown.at(-1)}, and others` : shown.at(-1)}`;
}
const at = (found: readonly (number | null)[]) => { const where = lines(found); return where ? ` (${where})` : ""; };

const otherNotices: Record<string, string> = {
  "work-limit": "The library is too large to read in full.",
  "cloud-synced-directory": "It is in a cloud-synced folder, so changes can arrive late.",
  "citation-keys-unavailable": "Better BibTeX keys could not be read, so some items have none.",
};

/** Where to look: which entries were skipped and why, then lines to check.
 * Keys are the person's own and shown only to them. */
function readingDetail(status: Reading) {
  const notices = reading(status);
  if (notices.length === 0) return null;
  const repeated = new Map<string, (number | null)[]>();
  for (const { code, key, line } of notices) if (code === "duplicate-key") repeated.set(key ?? "", [...repeated.get(key ?? "") ?? [], line]);
  const unreadable = notices.filter(({ code }) => code === "syntax").map(({ line }) => line);
  const skipped = notices.filter(({ code }) => skips(code)).length;
  const keys = [...repeated];
  const reasons = [
    ...keys.slice(0, 3).map(([key, found]) => `${numbers.format(found.length)} use the key ${isolate(key)}${at(found)}`),
    ...keys.length > 3 ? [`${count(keys.slice(3).reduce((sum, [, found]) => sum + found.length, 0), "other")} share a key`] : [],
    ...unreadable.length > 0 ? [`${skipped === 1 ? "it" : numbers.format(unreadable.length)} could not be read${at(unreadable)}`] : [],
  ];
  const check = notices.filter(({ code }) => !skips(code) && !(code in otherNotices)).map(({ line }) => line);
  const sentences = [
    skipped > 0 ? `${count(skipped, "entry", "entries")} skipped: ${reasons.join(", ")}.` : null,
    check.length > 0 ? `Check ${lines(check) ?? "the library"}.` : null,
    ...[...new Set(notices.map(({ code }) => otherNotices[code]))].filter((text): text is string => text !== undefined),
    notices.length < status.warnings ? say("library-more-not-listed", { count: status.warnings - notices.length }) : null,
  ].filter((text): text is string => text !== null);
  return sentences.join(" ");
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
function shortAge(at: number, now: number) {
  const seconds = Math.max(0, (now - at) / 1000);
  if (seconds < 45) return "now";
  if (seconds < 3600) return `${Math.max(1, Math.round(seconds / 60))} min`;
  if (seconds < 86400) return `${Math.round(seconds / 3600)} h`;
  return `${Math.round(seconds / 86400)} d`;
}

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

/** Words that tell similar result rows apart (#492). */
export const rowText = {
  /** Follows the first author's name in Compact rows when there are more. */
  etAl: say("row-et-al"),
  /** Short word for a CSL item type, read by screen readers; other types say none. */
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
  title: say("copied-notice-title"),
  /** Every "copied, not pasted" sentence says why and ends with its fix.
   * Written with ⌘V; noticeTitle shows Ctrl+V on other systems. The words
   * are the Notices table in docs/design/delivery.md, which a test reads. */
  pasted: (app: string | null) => (app ? `Pasted into ${app}` : "Pasted"),
  /** Keyed by the destination's copy reason codes; every copy has one. */
  reasons: {
    "paste-off": "Copied. Pasting is off in Settings. Press ⌘V to paste.",
    "unsupported-system": "Copied. Ibis can’t paste on this system yet. Press ⌘V to paste.",
    "no-target": "Copied. There was no app to paste into. Press ⌘V to paste.",
    "unknown-app": "Copied. Automatic paste is not approved for this app. Press ⌘V to paste.",
    "copy-only": "Copied. Copy Only is set for this app. Press ⌘V to paste.",
    "nonstandard-shortcut": "Copied. This app’s paste key does something else. Use its own paste command.",
    "target-changed": "Copied. The target app or window changed. Press ⌘V to paste.",
    "clipboard-changed": "Copied. The clipboard changed since. Copy the citation again.",
    "paste-expired": "Copied. The paste expired. Press ⌘V to paste.",
    "no-permission": "Copied. Ibis needs your OK to paste. Press ⌘V to paste.",
    "accessibility-required": "Copied. Ibis needs Accessibility access to paste safely. Press ⌘V to paste.",
    "terminal": "Copied. Not pasted into a terminal. Press ⌘V to paste.",
    "remote-desktop": "Copied. Not pasted into a remote desktop. Press ⌘V to paste.",
    "virtual-machine": "Copied. Not pasted into a virtual machine. Press ⌘V to paste.",
    "secure-field": "Copied. This field hides what you type. Press ⌘V to paste.",
    "secure-input": "Copied. Secure typing is on, possibly in another app. Press ⌘V to paste.",
    "paste-unavailable": "Copied. Ibis couldn’t confirm where the text would go. Press ⌘V to paste.",
    "elevated-app": "Copied. Not pasted into an administrator app. Press ⌘V to paste.",
    "key-release-failed": "Copied. Ibis couldn’t release the keys it pressed. Press and release Ctrl and V to paste.",
    "selection-not-shown": "Copied. Ibis replaces only text it showed you. Press ⌘V to replace it.",
    "citation-not-selected": "Copied. Ibis replaces a citation only when it is selected. Select it, then press ⌘V.",
  } as Record<string, string>,
  pluginNote: say("copied-notice-plugin-note"),
  source: say("copied-notice-source"),
  noticeFailed: say("copied-notice-notice-failed"),
};

export const updateText = {
  title: say("update-title"),
  downloading: say("update-downloading"),
  installing: say("update-installing"),
  failed: say("update-failed"),
};
