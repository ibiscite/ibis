import type React from "react";
import * as nativeApi from "./api";
import { copiedNoticeText, copyFormatNames, densityNames, keyText, libraryMessages, loginItemsPlace, loginItemText, operationFailures as FAILURE_MESSAGES, pickerText, rowText } from "./messages";
import { ariaKeys, KEY_TABLE, keyLabel, keysFor, type KeyBinding } from "./keys";
import { useEffect, useMemo, useRef, useState } from "react";
import { LibrarySetup, shortcutLabel } from "./LibrarySetup";
import { LibraryDetail, LibraryStatusLine, zoteroLibraryNames } from "./LibraryStatusLine";
import { usePointerSelection } from "./pointerSelection";
import { formatCitation, FORMATTED_STYLES, provisional, type FormattedCitation } from "./format/formatted";
import { STYLE_NAMES, type StyleId } from "./format/styles";
import { render } from "./format/markup";
import { afterCopy, rowClickAction } from "./copyFeedback";
import { KeysDialog } from "./features/picker/KeysDialog";
import { MenuItem, MoreMenu } from "./features/picker/MoreMenu";
import { AddGlyph, TypeIcon } from "./icons";
import {
  COPY_FORMATS,
  type Copied,
  type CopyFormat,
  type Density,
  type LibraryStatus,
  type LoginItem,
  type PickerPreferences,
  type RecordId,
  type SearchResult,
  type ShortcutStatus,
  type ZoteroGroup,
  type ZoteroLibrary,
} from "./api";

/** True while an input method is composing text; Enter then confirms the text.
 * Some WebKit versions report the confirming Enter with key code 229 only. */
function composing(e: React.KeyboardEvent): boolean {
  return e.nativeEvent.isComposing || e.keyCode === 229;
}

/** Selection is keyed by record identity, never by list position or citation key. */
function choiceKey(records: SearchResult[]): string {
  return JSON.stringify(records.map((record) => [record.id.source, record.id.native_key, record.revision]));
}

function idKey(id: RecordId): string {
  return `${id.source}\u0000${id.native_key}`;
}

type Operation = keyof typeof FAILURE_MESSAGES;

const DENSITIES: Density[] = ["compact", "comfortable", "spacious"];

// A search comes back if the picker returns within a minute, as in Raycast.
const KEEP_SEARCH_MS = 60_000;
const SAVE_BEFORE_QUIT_MS = 2_000;

// Shortcut labels follow the platform: ⌘ on macOS, Ctrl elsewhere.
const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);
const label = (accelerator: string) => keyLabel(accelerator, isMac);
// Ctrl+K is "delete to end of line" in macOS text fields, so there only ⌘K opens More.
const commandKey = (e: React.KeyboardEvent) => (isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey);

/** Typing a character there enters text, so a bare `?` is not a command. */
function typesText(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable || target instanceof HTMLTextAreaElement) return true;
  return target instanceof HTMLInputElement && !["button", "checkbox", "radio", "range", "color", "file", "submit", "reset"].includes(target.type);
}

/** `?` outside a text field, or Ctrl/⌘ with / (or ?) anywhere. */
function isKeysShortcut(e: React.KeyboardEvent): boolean {
  if (e.altKey) return false;
  if ((e.key === "/" || e.key === "?") && commandKey(e)) return true;
  return e.key === "?" && !e.ctrlKey && !e.metaKey && !typesText(e.target);
}

// Fields a row already shows; a match anywhere else is named under the row.
const VISIBLE_FIELDS = new Set(["title", "author", "citekey", "year"]);

/** The strongest match the row does not show, if every match is hidden. */
function hiddenMatch(matched: string[]): string | null {
  if (matched.length === 0 || matched.some((field) => VISIBLE_FIELDS.has(field))) return null;
  return matched[0] ?? null;
}

/** Rows name one or two authors in full and shorten three or more to
 * "García et al.". The count comes from the record's creators, so a name with
 * a comma is one; with one or two creators the joined surnames are the short form. */
function abbreviatedAuthors(r: SearchResult): boolean {
  return Boolean(r.first_author) && r.author_count > 2;
}

/** "et al." in its own isolate, so it keeps its reading order beside a right-to-left name. */
const etAl = <bdi>{rowText.etAl}</bdi>;

/** "García 2021" for the tile's tooltip: first author and year, else the key. */
function tileName(r: SearchResult): string {
  const first = r.first_author?.trim();
  return first ? (r.year === null ? first : `${first} ${r.year}`) : r.key;
}

// A stable provider lets the development gallery use synthetic data without IPC.
// Formatted citations are offered only when the provider can supply them.
export type PickerApi = Pick<typeof nativeApi,
  "chooseLibrary" | "useZotero" | "libraryStatus" | "reloadLibrary" | "copyCitation" | "copyFormat" | "pickerPreferences" | "setPickerPreferences" |
  "hidePicker" | "setPinned" | "onLibraryChanged" | "onPickerShown" | "onPickerHidden" | "quit" | "search" | "shortcutStatus"> &
  Partial<Pick<typeof nativeApi, "cslItems" | "copyFormatted" | "useZoteroLibraries" | "zoteroGroups" | "findSources" | "openFoundLibrary" | "showCopiedNotice" | "loginItem" | "setLoginItem">>;

/** A citation-key syntax, or a citation style written as `style:<id>`. */
export type Choice = CopyFormat | `style:${StyleId}`;

/** The one word the closed format control shows. */
function choiceName(choice: Choice): string {
  const style = styleOf(choice);
  return style ? STYLE_NAMES[style] : copyFormatNames[choice as CopyFormat];
}

function sameLibrary(a: ZoteroLibrary, b: ZoteroLibrary) {
  return a === b || (typeof a === "object" && typeof b === "object" && a.group === b.group);
}

function styleOf(choice: Choice): StyleId | null {
  return choice.startsWith("style:") ? (choice.slice("style:".length) as StyleId) : null;
}

export type PickerPreview = { query?: string; copied?: Copied };

// The picker: search, choose one or more references, copy them as a citation.
export function App({ api = nativeApi, preview }: { api?: PickerApi; preview?: PickerPreview } = {}) {
  const { chooseLibrary, useZotero, libraryStatus, reloadLibrary, copyCitation, copyFormat,
    hidePicker, setPinned, onLibraryChanged, onPickerShown, onPickerHidden, quit, search, shortcutStatus, cslItems, copyFormatted,
    pickerPreferences, setPickerPreferences, useZoteroLibraries, zoteroGroups } = api;
  const { findSources, openFoundLibrary, showCopiedNotice, loginItem, setLoginItem } = api;
  const setupApi = useMemo(() => ({ chooseLibrary, useZotero, findSources, openFoundLibrary }),
    [chooseLibrary, useZotero, findSources, openFoundLibrary]);
  const [query, setQuery] = useState(preview?.query ?? "");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [announcement, setAnnouncement] = useState("");
  // The query the shown results belong to; differs while a search is pending.
  const [resultsQuery, setResultsQuery] = useState<string | null>(null);
  const [resultsRevision, setResultsRevision] = useState<string | null>(null);
  const [active, setActive] = useState(0);
  const pointer = usePointerSelection(active, setActive);
  const [selected, setSelected] = useState<Map<string, SearchResult>>(new Map());
  const [selectionChanged, setSelectionChanged] = useState(false);
  const [format, setFormat] = useState<Choice>("pandoc");
  const style = cslItems && copyFormatted ? styleOf(format) : null;
  // The formatted preview, tagged with the style, choice and library it was
  // made for, so Enter never copies a preview of an earlier choice.
  const [formatted, setFormatted] = useState<{ key: string; result: FormattedCitation | "failed" | "stale" } | null>(null);
  const [copied, setCopied] = useState<Copied | null>(preview?.copied ?? null);
  const [error, setError] = useState<string | null>(null);
  const [copyError, setCopyError] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState<ShortcutStatus | null>(null);
  // Changes whenever a library opens, so the current query is searched again.
  const [libraryVersion, setLibraryVersion] = useState(0);
  const statusReads = useRef(0);
  // Undefined while loading; null means the status request itself failed.
  const [library, setLibrary] = useState<LibraryStatus | null>();
  const input = useRef<HTMLInputElement>(null);
  const moreButton = useRef<HTMLButtonElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [keysOpen, setKeysOpen] = useState(false);
  // Where focus goes back to when the keys overlay closes.
  const keysReturn = useRef<HTMLElement | null>(null);
  // While Alt (⌥) is held, the footer shows the other keys instead of Enter.
  const [altHeld, setAltHeld] = useState(false);
  // Enter copies only from the search box; elsewhere it presses the focused control.
  const [searchFocused, setSearchFocused] = useState(false);
  // autoFocus can happen before React listens for focus events.
  useEffect(() => setSearchFocused(document.activeElement === input.current), []);
  const [pinned, setPinnedState] = useState(false);
  // When the picker last went away; the search waits a minute, in memory only.
  const hiddenAt = useRef<number | null>(null);
  // A search Escape cleared comes back on reopening within the same minute.
  const escapedQuery = useRef<string | null>(null);
  const [restored, setRestored] = useState(0);
  const [failures, setFailures] = useState<Partial<Record<Operation, boolean>>>({});
  const report = (operation: Operation, failed = true) => {
    setFailures((previous) => ({ ...previous, [operation]: failed }));
  };
  // Setup failures belong to the setup screen; they go when a library is chosen.
  const setupDone = library !== undefined && library?.state !== "none";
  useEffect(() => {
    if (setupDone) setFailures(({ findSources: _found, openFound: _opened, ...rest }) => rest);
  }, [setupDone]);
  const perform = async (operation: Operation, action: () => Promise<unknown>) => {
    try { await action(); report(operation, false); }
    catch { report(operation); }
  };
  // A click still waiting for its preview must never copy once the user has left.
  const [clickCopy, setClickCopy] = useState<string | null>(null);
  const hide = () => {
    setClickCopy(null);
    void perform("hide", hidePicker);
  };
  const clearSearch = () => {
    escapedQuery.current = null;
    setQuery("");
    setSelected(new Map());
    setActive(0);
  };
  // Saved preferences, once read; choices made before that wait in `pending`
  // so a slow read never overwrites them and a failed read never resets the
  // file to defaults.
  const [density, setDensity] = useState<Density>("comfortable");
  const [largerText, setLargerText] = useState(false);
  const savedPreferences = useRef<PickerPreferences | null>(null);
  const pending = useRef<Partial<PickerPreferences>>({});
  const pendingKeyFormat = useRef<{ format: CopyFormat; saved: boolean } | null>(null);
  const preferenceSave = useRef<Promise<unknown>>(Promise.resolve());
  const preferenceLoad = useRef<Promise<void>>(Promise.resolve());
  const savePreferences = (change: Partial<PickerPreferences>, keyFormat: CopyFormat | null = null) => {
    if (keyFormat !== null) pendingKeyFormat.current = { format: keyFormat, saved: false };
    pending.current = { ...pending.current, ...change };
    if (!savedPreferences.current) return;
    const next = { ...savedPreferences.current, ...pending.current } as PickerPreferences;
    const chosenKey = pendingKeyFormat.current;
    pending.current = {};
    savedPreferences.current = next;
    // Preserve choice order and retry an unsaved key. Queued density/style
    // changes do not rewrite a key whose earlier save already succeeded.
    const save = preferenceSave.current.catch(() => undefined)
      .then(() => setPickerPreferences(next, chosenKey && !chosenKey.saved ? chosenKey.format : null))
      .then(() => {
        if (chosenKey) chosenKey.saved = true;
        if (pendingKeyFormat.current === chosenKey) pendingKeyFormat.current = null;
      });
    preferenceSave.current = save;
    void perform("preferences", () => save);
  };
  const quitAfterSaving = async () => {
    // Drain choices already made, including ones awaiting the initial read.
    // A failed or stuck settings command must never trap someone in the app.
    let timer: number | undefined;
    await Promise.race([
      preferenceLoad.current.then(() => preferenceSave.current).catch(() => undefined),
      new Promise<void>((resolve) => { timer = window.setTimeout(resolve, SAVE_BEFORE_QUIT_MS); }),
    ]);
    window.clearTimeout(timer);
    await quit();
  };
  const chooseFormat = (choice: Choice) => {
    // A new format choice cancels the earlier click waiting for its preview.
    setClickCopy(null);
    setFormat(choice);
    const chosenStyle = styleOf(choice);
    savePreferences({ citation_style: chosenStyle }, chosenStyle ? null : choice as CopyFormat);
  };
  const changeDensity = () => {
    const next = DENSITIES[(DENSITIES.indexOf(density) + 1) % DENSITIES.length] ?? "comfortable";
    setDensity(next);
    savePreferences({ density: next });
  };
  const toggleLargerText = () => {
    setLargerText(!largerText);
    savePreferences({ larger_text: !largerText });
  };
  useEffect(() => {
    document.documentElement.dataset.density = density;
    document.documentElement.dataset.textSize = largerText ? "larger" : "normal";
    return () => {
      delete document.documentElement.dataset.density;
      delete document.documentElement.dataset.textSize;
    };
  }, [density, largerText]);

  useEffect(() => {
    // Only Alt on its own, or Alt still down from that, counts; any other key ends it.
    const down = (e: KeyboardEvent) => setAltHeld((held) => e.key === "Alt" || (e.altKey && held));
    const up = (e: KeyboardEvent) => { if (e.key === "Alt" || !e.altKey) setAltHeld(false); };
    const reset = () => setAltHeld(false);
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", reset);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", reset);
    };
  }, []);

  // Whether Ibis starts at login, as the system reports it; read again each
  // time the picker opens, since people also change it in system settings.
  const [startAtLogin, setStartAtLogin] = useState<LoginItem | null>(null);
  // Calls run one after another, and only the newest one's answer is shown,
  // so quick clicks end in the state of the last click.
  const loginQueue = useRef<Promise<unknown>>(Promise.resolve());
  const loginRequest = useRef(0);
  const askLoginItem = (operation: "loginItem" | "loginItemChange", call: () => Promise<LoginItem>, failed?: () => void) => {
    const request = ++loginRequest.current;
    const run = loginQueue.current.catch(() => undefined).then(call).then(
      (item) => { if (request === loginRequest.current) setStartAtLogin(item); },
      (error: unknown) => { if (request === loginRequest.current) failed?.(); throw error; },
    );
    loginQueue.current = run;
    void perform(operation, () => run);
  };
  const readLoginItem = () => {
    if (loginItem) askLoginItem("loginItem", loginItem);
  };
  const toggleStartAtLogin = () => {
    if (!setLoginItem || !startAtLogin) return;
    const before = startAtLogin;
    const on = !before.enabled;
    // Shown at once; the system's answer replaces it.
    setStartAtLogin({ ...before, enabled: on, problem: null });
    askLoginItem("loginItemChange", () => setLoginItem(on), () => setStartAtLogin({ ...before, problem: null }));
  };
  useEffect(() => { if (menuOpen) readLoginItem(); }, [menuOpen]);
  const loginItemOffered = Boolean(setLoginItem) && startAtLogin !== null && startAtLogin.problem !== "unsupported";

  const togglePin = () => {
    const on = !pinned;
    setPinnedState(on);
    // Show the pin as it really is if the window could not change.
    void perform("pin", () => setPinned(on).catch((error: unknown) => { setPinnedState(!on); throw error; }));
  };

  // When the picker is shown, put the cursor in the search box with the
  // previous query selected, so typing replaces it.
  useEffect(() => {
    let acceptInitialStatus = true;
    let mounted = true;
    const subscriptionFailure = (operation: Operation) => {
      if (mounted) report(operation);
      else console.error(FAILURE_MESSAGES[operation]);
      return () => undefined;
    };
    const focusSearch = () => {
      if (hiddenAt.current !== null && performance.now() - hiddenAt.current > KEEP_SEARCH_MS) clearSearch();
      else if (hiddenAt.current !== null && escapedQuery.current !== null) {
        const escaped = escapedQuery.current;
        escapedQuery.current = null;
        setQuery((now) => (now === "" ? escaped : now));
        setRestored((n) => n + 1);
      }
      hiddenAt.current = null;
      readLoginItem();
      input.current?.focus();
      input.current?.select();
    };
    // Every hide is reported, including the native one when focus moves away;
    // a monotonic clock keeps clock changes from clearing or keeping a search.
    const unlistenHidden = onPickerHidden(() => {
      hiddenAt.current = performance.now();
      setKeysOpen(false);
      setAltHeld(false);
      setClickCopy(null);
    })
      .catch(() => subscriptionFailure("pickerEvents"));
    const unlisten = onPickerShown(focusSearch).catch(() => subscriptionFailure("pickerEvents"));
    const unlistenLibrary = onLibraryChanged((status) => {
      statusReads.current += 1;
      acceptInitialStatus = false;
      setLibrary(status);
      report("status", false);
      setLibraryVersion((v) => v + 1);
    }).catch(() => subscriptionFailure("libraryEvents"));
    libraryStatus()
      .then((status) => { if (acceptInitialStatus) setLibrary(status); })
      .catch(() => { if (acceptInitialStatus) { setLibrary(null); report("status"); } });
    readLoginItem();
    shortcutStatus()
      .then(setShortcut)
      .catch(() => { setShortcut(null); report("shortcut"); });
    // The saved style wins over the saved key format; a choice made while
    // these load wins over both.
    preferenceLoad.current = Promise.allSettled([copyFormat(), pickerPreferences()]).then(([key, saved]) => {
      if (!mounted) return;
      if (key.status === "rejected") report("format");
      if (saved.status === "rejected") {
        // Without saved preferences the saved key format still applies.
        report("preferences");
        if (pending.current.citation_style === undefined && key.status === "fulfilled") setFormat(key.value);
        return;
      }
      const loaded = { ...saved.value, ...pending.current } as PickerPreferences;
      savedPreferences.current = saved.value;
      setDensity(loaded.density);
      setLargerText(loaded.larger_text);
      const chosen = pending.current.citation_style !== undefined;
      if (Object.keys(pending.current).length > 0) savePreferences({});
      if (chosen) return;
      const style = saved.value.citation_style;
      if (style && cslItems && copyFormatted) setFormat(`style:${style}`);
      else if (key.status === "fulfilled") setFormat(key.value);
    });
    return () => {
      acceptInitialStatus = false;
      mounted = false;
      void unlistenHidden.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
      void unlisten.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
      void unlistenLibrary.then((stop) => stop()).catch(() => console.error("Library event cleanup failed."));
    };
  }, []);

  useEffect(() => {
    setAnnouncement("");
    // Typing before choosing a library is normal; preserve the query and
    // chooser. Actual library/status failures still report search errors.
    if (library === undefined || library?.state === "none" || library?.state === "loading" || (library?.state === "error" && !library.has_snapshot)) {
      setResults([]);
      setResultsQuery(query);
      setActive(0);
      setError(null);
      return;
    }
    let current = true;
    let announcementTimer: number | undefined;
    search(query)
      .then(({ revision, results: found }) => {
        if (current) {
          setResults(found);
          setResultsRevision(revision);
          if (revision !== library?.revision) refreshSnapshot();
          setResultsQuery(query);
          setActive(0);
          setError(null);
          announcementTimer = window.setTimeout(() => {
            setAnnouncement(pickerText.resultCount(found.length));
          }, 300);
        }
      })
      .catch(() => {
        // Results of an earlier query must not sit beside this query's error.
        if (current) {
          setResults([]);
          setResultsQuery(query);
          setError(pickerText.searchError);
        }
      });
    return () => {
      current = false;
      window.clearTimeout(announcementTimer);
    };
  }, [query, library, libraryVersion]);

  // A restored search is selected once it is back in the box, so typing replaces it.
  useEffect(() => {
    if (restored > 0) input.current?.select();
  }, [restored]);

  // Keep the highlighted result visible while moving with the arrow keys.
  // A row chosen by the pointer is already under it; scrolling would move it away.
  useEffect(() => {
    if (pointer.fromPointer.current) {
      pointer.fromPointer.current = false;
      return;
    }
    document.getElementById(`result-${active}`)?.scrollIntoView?.({ block: "nearest" });
  }, [active, pointer.fromPointer]);

  // Success is brief; warnings stay available until explicitly dismissed.
  useEffect(() => {
    if (preview || !copied || copied.warnings.length > 0) return;
    const timer = window.setTimeout(() => setCopied(null), 2400);
    return () => window.clearTimeout(timer);
  }, [copied, preview]);

  const toggle = (result: SearchResult) => {
    setSelectionChanged(true);
    setSelected((previous) => {
      const next = new Map(previous);
      if (!next.delete(idKey(result.id))) next.set(idKey(result.id), result);
      return next;
    });
  };

  // Results belong to the query that produced them. While a newer search is
  // pending, the rows still on screen are not acted on or offered by any key,
  // click or footer hint; only these current results are.
  const current = resultsQuery === query && resultsRevision === library?.revision && results.every((record) => record.revision === resultsRevision) ? results : [];
  // The selection, or the highlighted result when nothing is selected.
  const chosen = selected.size > 0 ? [...selected.values()] : current.slice(active, active + 1);
  const chosenKey = choiceKey(chosen);
  const chosenRevision = chosen[0]?.revision;
  const staleSelection = chosen.length > 0 && (!chosenRevision || chosen.some((record) => record.revision !== chosenRevision) || chosenRevision !== library?.revision);
  const previewKey = `${style}\u0002${libraryVersion}\u0002${chosenKey}`;
  // What Enter would copy now; null while it is being formatted.
  const shown = staleSelection ? "stale" : formatted?.key === previewKey ? formatted.result : null;
  const refreshSnapshot = () => {
    const request = ++statusReads.current;
    void libraryStatus().then((status) => {
      if (statusReads.current !== request) return;
      setLibrary(status);
      setLibraryVersion((version) => version + 1);
      report("status", false);
    }).catch(() => { if (statusReads.current === request) report("status"); });
  };

  // Format the preview again whenever the choice or the style changes.
  useEffect(() => {
    if (!style || !cslItems || chosen.length === 0 || staleSelection || !chosenRevision) return;
    let live = true;
    const key = previewKey;
    cslItems(chosen.map((r) => r.id), chosenRevision)
      .then((csl) => formatCitation(csl, style))
      .then((result) => { if (live) setFormatted({ key, result }); })
      .catch((error) => { if (live) {
        const stale = nativeApi.isSnapshotChanged(error);
        setFormatted({ key, result: stale ? "stale" : "failed" });
        if (stale) refreshSnapshot();
      } });
    return () => { live = false; };
  }, [previewKey]);

  // Copy the chosen references in the chosen format.
  const copy = async () => {
    if (chosen.length === 0) return;
    setCopied(null);
    setCopyError(null);
    if (shown === "stale" || !chosenRevision) {
      setCopyError(pickerText.snapshotChanged);
      refreshSnapshot();
      return;
    }
    let outcome: Copied;
    try {
      if (style && copyFormatted) {
        // Copy only what the preview shows, never a result still being made.
        if (shown === null || shown === "failed") {
          setCopyError(shown === null ? pickerText.previewNotReady : pickerText.previewError);
          return;
        }
        await copyFormatted(shown.text, shown.html, chosenRevision);
        outcome = { text: shown.text, warnings: shown.warnings };
      } else {
        outcome = await copyCitation(
          chosen.map((r) => r.id),
          format as CopyFormat,
          chosenRevision,
        );
      }
    } catch (error) {
      const stale = nativeApi.isSnapshotChanged(error);
      setCopyError(stale ? pickerText.snapshotChanged : pickerText.copyError);
      if (stale) refreshSnapshot();
      return;
    }
    setCopied(outcome);
    setSelected(new Map());
    // After a copy, reopening keeps the current search, not one cleared before it.
    escapedQuery.current = null;
    // Get out of the way so the user can paste, unless there is something to
    // read or the picker is pinned beside the document.
    if (afterCopy(outcome, pinned) === "hide-and-notify") {
      void perform("hide", async () => {
        await hidePicker();
        // The notice is extra; a failure to show it never undoes the copy.
        await showCopiedNotice?.(outcome.text).catch(() => console.error(copiedNoticeText.noticeFailed));
      });
    }
  };
  // A click on a row copies what Enter would copy then (the row, or the
  // citation being built), once that is ready; anything changing first cancels it.
  useEffect(() => {
    if (clickCopy === null) return;
    if (chosenKey !== clickCopy) {
      setClickCopy(null);
      return;
    }
    if (style && shown === null) return;
    setClickCopy(null);
    void copy();
  }, [clickCopy, chosenKey, shown]);
  const pressedAt = useRef<{ x: number; y: number } | null>(null);

  const busy = library?.state === "loading" || library?.state === "reloading";
  const loadingMessage = library?.state === "reloading"
    ? libraryMessages.reloading(library.name)
    : libraryMessages.opening(library?.name ?? null);

  // The search box is the place to type; the shortcut works from anywhere in the picker.
  const isMenuShortcut = (e: React.KeyboardEvent) =>
    e.key.toLowerCase() === "k" && commandKey(e) && !e.altKey && !e.shiftKey;
  // Zotero libraries: the ones chosen, and the groups Zotero offers, listed
  // when the menu opens or when results come from several libraries.
  const [groups, setGroups] = useState<ZoteroGroup[] | null>(null);
  // A choice sent but not yet reported back, so a second toggle builds on it.
  const [pendingLibraries, setPendingLibraries] = useState<ZoteroLibrary[] | null>(null);
  const reportedLibraries = library?.zotero_libraries ?? [];
  const sameChoice = (a: ZoteroLibrary[], b: ZoteroLibrary[]) =>
    a.length === b.length && a.every((choice, i) => sameLibrary(choice, b[i]!));
  useEffect(() => {
    if (pendingLibraries && sameChoice(pendingLibraries, reportedLibraries)) setPendingLibraries(null);
  }, [library]);
  const chosenLibraries = pendingLibraries ?? reportedLibraries;
  const severalLibraries = chosenLibraries.length > 1;
  // Nothing matched this search, so the empty state offers what to try next.
  const nothingMatched = Boolean(query) && resultsQuery === query && results.length === 0 && !error;
  const wantGroups = Boolean(zoteroGroups && library?.zotero && (menuOpen || severalLibraries || nothingMatched));
  useEffect(() => {
    // Group names can refresh in the background, but a failure belongs only
    // to this menu opening. Closing it also invalidates its pending request.
    report("zoteroGroups", false);
    if (!wantGroups || !zoteroGroups) return;
    let current = true;
    zoteroGroups()
      .then((list) => { if (current) { setGroups(list); report("zoteroGroups", false); } })
      .catch(() => { if (current && menuOpen) report("zoteroGroups"); });
    return () => { current = false; };
  }, [wantGroups, menuOpen, libraryVersion]);
  const isChosen = (choice: ZoteroLibrary) => chosenLibraries.some((chosen) => sameLibrary(chosen, choice));
  const toggleLibrary = (choice: ZoteroLibrary) => {
    if (!useZoteroLibraries) return;
    const next = isChosen(choice)
      ? chosenLibraries.filter((chosen) => !sameLibrary(chosen, choice))
      : [...chosenLibraries, choice];
    if (next.length === 0) return;
    setPendingLibraries(next);
    void perform("zotero", () => useZoteroLibraries(next).catch((error: unknown) => {
      setPendingLibraries(null);
      throw error;
    }));
  };
  // Every Zotero library on offer, once the group list is known.
  const allLibraries = (): ZoteroLibrary[] => ["user", ...offeredGroups.map((group) => ({ group }))];
  const searchAllLibraries = () => {
    if (!useZoteroLibraries) return;
    const all = allLibraries();
    setPendingLibraries(all);
    input.current?.focus();
    void perform("zotero", () => useZoteroLibraries(all).catch((error: unknown) => {
      setPendingLibraries(null);
      throw error;
    }));
  };
  const groupName = (id: number) => groups?.find((group) => group.id === id)?.name ?? pickerText.groupLibrary(id);
  // Read when More opens, the only time it is shown there.
  const libraryDetail = menuOpen ? libraryMessages.detail(library, Date.now(), (status) => zoteroLibraryNames(status, groupName)) : null;
  // Every group Zotero lists, plus chosen groups it no longer lists, so they can be turned off.
  const offeredGroups = [...(groups?.map((group) => group.id) ?? []),
    ...chosenLibraries.flatMap((chosen) => (typeof chosen === "object" ? [chosen.group] : []))]
    .filter((id, i, all) => all.indexOf(id) === i);
  const libraryOf = (source: string) =>
    source === "zotero:user" ? pickerText.myLibrary
      : source.startsWith("zotero:group:") ? groupName(Number(source.slice("zotero:group:".length))) : null;
  /** Names the library too when another chosen reference has the same key. */
  const removeLabel = (r: SearchResult) => {
    const library = libraryOf(r.id.source);
    const shared = [...selected.values()].some((other) => other.key === r.key && other.id.source !== r.id.source);
    return shared && library ? pickerText.removeReferenceIn(r.key, library) : pickerText.removeReference(r.key);
  };
  const closeMenu = (refocus: boolean) => {
    setMenuOpen(false);
    if (refocus) moreButton.current?.focus();
  };
  const runFromMenu = (operation: Operation, action: () => Promise<unknown>) => {
    closeMenu(false);
    input.current?.focus();
    void perform(operation, action);
  };
  const narrowed = Boolean(library?.zotero && useZoteroLibraries && groups) && allLibraries().some((choice) => !isChosen(choice));
  const openKeys = () => {
    closeMenu(false);
    const focused = document.activeElement;
    // A menu item is gone once the menu closes; the search box takes its place.
    keysReturn.current = focused instanceof HTMLElement && !focused.closest("[role='menu']") ? focused : input.current;
    setKeysOpen(true);
  };
  const closeKeys = () => {
    setKeysOpen(false);
    const back = keysReturn.current;
    (back?.isConnected ? back : input.current)?.focus();
  };
  // The keys overlay lists the global shortcut too, when it works.
  const bindings: readonly KeyBinding[] = shortcut?.registered
    ? [...KEY_TABLE, { region: "system", action: "open", keys: [shortcut.shortcut] }]
    : KEY_TABLE;
  // What Enter does now, from the same state the copy uses; null when it does nothing.
  // A formatted citation still being made, or one that failed, is not copied.
  const enterAction = !searchFocused || chosen.length === 0 || shown === "stale" || (style && (shown === null || shown === "failed")) ? null
    : chosen.length > 1 ? keyText.copySeveralAs(chosen.length, choiceName(format)) : keyText.copyAs(choiceName(format));
  const highlighted = current[active];
  const shiftEnterAction = searchFocused && highlighted
    ? (selected.has(idKey(highlighted.id)) ? keyText.removeFromCitation : keyText.addToCitation) : null;
  const escapeAction = menuOpen ? keyText.closeMenu : query ? keyText.clearSearch : keyText.hide;

  return (
    <main
      onFocus={(e) => setSearchFocused(e.target === input.current)}
      onBlur={() => setSearchFocused(false)}
      onKeyDown={(e) => {
        if (composing(e)) return;
        if (isKeysShortcut(e)) {
          e.preventDefault();
          openKeys();
        } else if (e.key === "Escape") {
          e.preventDefault();
          setClickCopy(null);
          // Escape steps back one layer: the menu, then the search, then the
          // picker. Chosen references stay; hiding keeps them for a minute.
          if (menuOpen) closeMenu(true);
          else if (query) {
            escapedQuery.current = query;
            setQuery("");
            input.current?.focus();
          } else hide();
        } else if (isMenuShortcut(e)) {
          e.preventDefault();
          if (menuOpen) closeMenu(true);
          else setMenuOpen(true);
        } else if (e.key === "q" && (e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey) {
          e.preventDefault();
          void perform("quit", quitAfterSaving);
        }
      }}
    >
      {shortcut?.error && (
        <p role="status">
          {pickerText.shortcutError(shortcut.registered, shortcut.shortcut, shortcut.error)}
        </p>
      )}
      <header className="search-bar" data-tauri-drag-region title={pickerText.dragSearchBar}>
        <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor">
          <circle cx="10.5" cy="10.5" r="6.5" />
          <path d="m16 16 5 5" />
        </svg>
        <label className="visually-hidden" htmlFor="search">
          {pickerText.searchLabel}
        </label>
        <input
          id="search"
          type="search"
          placeholder={pickerText.searchPlaceholder}
          role="combobox"
          aria-autocomplete="list"
          aria-expanded={results.length > 0}
          aria-controls="results"
          aria-activedescendant={results[active] ? `result-${active}` : undefined}
          ref={input}
          autoFocus
          value={query}
          onChange={(e) => {
            escapedQuery.current = null;
            setQuery(e.target.value);
          }}
          onKeyDown={(e) => {
            if (composing(e)) return;
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              e.preventDefault();
              const step = e.key === "ArrowDown" ? 1 : -1;
              setActive((i) => Math.min(Math.max(i + step, 0), Math.max(results.length - 1, 0)));
            } else if (e.key === "Enter" && e.shiftKey) {
              e.preventDefault();
              const result = current[active];
              if (result) toggle(result);
            } else if (e.key === "Enter") {
              e.preventDefault();
              void copy();
            }
          }}
        />
        {/* The closed control shows one word; the list and screen readers get the full name. */}
        <span className="format-picker" title={pickerText.copyAs}>
          <span aria-hidden="true" className="format-name">{choiceName(format)}</span>
          <label className="visually-hidden" htmlFor="format">
            {pickerText.copyAs}
          </label>
          <select id="format" value={format} onChange={(e) => chooseFormat(e.target.value as Choice)}>
            <optgroup label={pickerText.keyFormats}>
              {COPY_FORMATS.map((f) => (
                <option key={f.value} value={f.value}>
                  {f.label}
                </option>
              ))}
            </optgroup>
            {cslItems && copyFormatted && (
              <optgroup label={pickerText.formattedStyles}>
                {FORMATTED_STYLES.map((s) => (
                  <option key={s.id} value={`style:${s.id}`}>
                    {s.title}
                  </option>
                ))}
              </optgroup>
            )}
          </select>
        </span>
        <button
          type="button"
          className="pin-button"
          aria-pressed={pinned}
          aria-label={pickerText.pin}
          title={pinned ? pickerText.unpinTitle : pickerText.pinTitle}
          onClick={togglePin}
        >
          <svg aria-hidden="true" viewBox="0 0 24 24" fill={pinned ? "currentColor" : "none"} stroke="currentColor">
            <path d="M9 4h6l-1 5 4 4H6l4-4z" />
            <path d="M12 13v7" />
          </svg>
        </button>
      </header>
      <div className="notices">
        {library?.settings_notice && <p role="alert">{library.settings_notice}</p>}
        {busy && <p role="status">{loadingMessage}</p>}
        {library?.error && <p role="alert">{libraryMessages.openError(library.name, library.error, library.zotero)}</p>}
        {(Object.keys(failures) as Operation[]).filter((operation) => failures[operation]).map((operation) => (
          <p role="alert" key={operation}>{FAILURE_MESSAGES[operation]}</p>
        ))}
        {startAtLogin?.problem && startAtLogin.problem !== "unsupported" && (
          <p role="alert">{loginItemText.problem(startAtLogin.problem, loginItemsPlace(navigator.platform), startAtLogin.name)}</p>
        )}
        {error && <p role="alert">{error}</p>}
        {copyError && <p role="alert">{copyError}</p>}
        {copied && (
          <div role="status" className="copy-notice">
            <p>
              {pickerText.copied} <code>{copied.text}</code>
            </p>
            {copied.warnings.length > 0 && (
              <>
                <button type="button" className="dismiss" aria-label={pickerText.dismissWarnings} onClick={() => setCopied(null)}>
                  {pickerText.dismiss}
                </button>
                <ul aria-label={pickerText.copyWarnings}>
                  {copied.warnings.map((w) => (
                    <li key={w}>{w}</li>
                  ))}
                </ul>
              </>
            )}
          </div>
        )}
      </div>
      <p className="visually-hidden" role="status" aria-label={pickerText.citationSelection} aria-live="polite" aria-atomic="true">
        {selectionChanged ? pickerText.selectionCount(selected.size) : ""}
      </p>
      {selected.size > 0 && (
        <ul aria-label={pickerText.selectedReferences}>
          {[...selected.entries()].map(([key, r]) => (
            <li key={key}>
              <code>{r.key}</code>{" "}
              <button type="button" onClick={() => toggle(r)} aria-label={removeLabel(r)}>
                {pickerText.remove}
              </button>
            </li>
          ))}
        </ul>
      )}
      {style && chosen.length > 0 && (
        <section aria-label={pickerText.preview} className="format-preview">
          {shown === null && <p role="status">{pickerText.previewPending}</p>}
          {shown === "failed" && <p role="alert">{pickerText.previewError}</p>}
          {shown === "stale" && <p role="alert">{pickerText.snapshotChanged}</p>}
          {shown && shown !== "failed" && shown !== "stale" && (
            <>
              <div className="preview-text">{render(shown.markup)}</div>
              {provisional(style) && <p className="provisional">{pickerText.provisional}</p>}
              {shown.warnings.length > 0 && (
                <ul aria-label={pickerText.copyWarnings}>
                  {shown.warnings.map((w, i) => <li key={i}>{w}</li>)}
                </ul>
              )}
            </>
          )}
        </section>
      )}
      <div className="results-area">
        <div className="results-heading" aria-hidden="true">
          <span>{query ? pickerText.bestMatches : pickerText.references}</span><span>{results.length > 0 ? pickerText.resultCount(results.length) : ""}</span>
        </div>
        <p className="visually-hidden" role="status" aria-label={pickerText.searchResults} aria-live="polite" aria-atomic="true">
          {announcement}
        </p>
        <ul id="results" role="listbox" aria-label={pickerText.results} onMouseMove={pointer.onMouseMove}>
          {results.map((r, i) => {
            const compact = density === "compact";
            // A match in the container is always named: the row may shorten the container.
            const why = hiddenMatch(r.matched);
            const itemType = density === "spacious" ? rowText.itemType(r.item_type) : null;
            const added = selected.has(idKey(r.id));
            return (
              <li
                key={idKey(r.id)}
                id={`result-${i}`}
                role="option"
                aria-selected={i === active}
                data-citation-selected={added || undefined}
                data-active={i === active || undefined}
                onPointerDown={(e) => { pressedAt.current = { x: e.clientX, y: e.clientY }; }}
                onClick={(e) => {
                  const start = pressedAt.current;
                  pressedAt.current = null;
                  // Rows of an earlier query, still shown while a newer search
                  // is pending, are not acted on by a click or Shift+click (#507).
                  if (current !== results) return;
                  const action = rowClickAction({
                    shiftKey: e.shiftKey, ctrlKey: e.ctrlKey, metaKey: e.metaKey, altKey: e.altKey, detail: e.detail,
                    moved: start ? Math.hypot(e.clientX - start.x, e.clientY - start.y) : 0,
                    onControl: (e.target as Element).closest("button, a, input, select, textarea, [role=button]") !== null,
                    selectedText: !(window.getSelection()?.isCollapsed ?? true),
                  });
                  if (action === "none") return;
                  setActive(i);
                  if (action === "toggle") toggle(r);
                  else setClickCopy(selected.size > 0 ? chosenKey : choiceKey([r]));
                  input.current?.focus();
                }}
              >
                {/* The leading tile: the type icon, which gives way to the add
                    (or remove) control on hover and on the active row. An option's
                    children are presentational (ARIA) and must not be focusable, so
                    the control is for the pointer only: no Tab stop, no role, its
                    name in the tooltip. Keyboards use Shift+Enter. */}
                <span className="result-tile">
                  <TypeIcon itemType={r.item_type} />
                  <span className="tile-button" aria-hidden="true"
                    title={added ? pickerText.removeNamed(tileName(r)) : pickerText.addNamed(tileName(r))}
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={(e) => {
                      // The row's own click (copy) does not run for the tile.
                      e.stopPropagation();
                      pressedAt.current = null;
                      if (current !== results) return;
                      setActive(i);
                      toggle(r);
                      input.current?.focus();
                    }}>
                    <AddGlyph added={added} />
                  </span>
                </span>
                <span className="result-body">
                  <span className="result-line">
                    <strong className="result-title">{r.title}</strong>
                    {compact && r.first_author && <span className="result-first-author" dir="auto">{r.first_author}{r.author_count > 1 && <> {etAl}</>}</span>}
                    <span className="result-year">{r.year ?? pickerText.noDate}</span>
                  </span>
                  <span className="result-meta">
                    {/* Each clipping part takes its own text direction, so Latin text in a
                        right-to-left row is cut at its end. The separator is its own part. */}
                    <span className="result-byline">
                      <span className="result-authors" dir="auto">
                        {abbreviatedAuthors(r) ? <><span aria-hidden="true">{r.first_author} {etAl}</span><span className="visually-hidden">{r.authors}</span></> : r.authors}
                      </span>
                      {!compact && r.container && r.authors && <span className="result-separator" aria-hidden="true">{"\u00a0· "}</span>}
                      {!compact && r.container && <span className="result-container" dir="auto">{r.container}</span>}
                    </span>
                    {itemType && <span className="result-type">{itemType}</span>}
                    <code className="result-key" dir="auto">{r.key}</code>
                    {severalLibraries && libraryOf(r.id.source) && <span className="result-library">{libraryOf(r.id.source)}</span>}
                  </span>
                  {why && <span className="result-why">{pickerText.matchedIn(why)}</span>}
                  {/* The type in words where the row does not show it; the tile's icon is decorative. */}
                  {!itemType && rowText.itemType(r.item_type) && <span className="visually-hidden">{rowText.itemType(r.item_type)}</span>}
                  {added && <span className="visually-hidden">{pickerText.addedToCitation}</span>}
                </span>
              </li>
            );
          })}
        </ul>
        {results.length === 0 && !error && library !== undefined && !busy && !library?.error && (
          <div className="empty-state">
            {library?.name ? (
              library.has_snapshot && library.references === 0
                ? <>
                  <p>{libraryMessages.empty(library.snapshot_name, library.zotero)}</p>
                  <button type="button" onClick={() => void perform("choose", chooseLibrary)}>{pickerText.chooseLibrary}</button>
                </>
                : query
                  ? <>
                    <p>{pickerText.noMatches}</p>
                    <p className="empty-hint">{pickerText.noMatchesHint}</p>
                    <div className="empty-actions">
                      <button type="button" onClick={() => { setQuery(""); input.current?.focus(); }}>{pickerText.clearSearch}</button>
                      {narrowed && <button type="button" onClick={searchAllLibraries}>{pickerText.searchAllLibraries}</button>}
                    </div>
                  </>
                  : <><p>{pickerText.typeToFind}</p><p className="empty-hint">{pickerText.typeToFindHint}</p></>
            ) : library?.state === "none" ? (
              <LibrarySetup api={setupApi} perform={perform}
                writing={{ format, formatted: Boolean(cslItems && copyFormatted), choose: chooseFormat }}
                shortcut={shortcut?.registered ? shortcutLabel(shortcut.shortcut, isMac) : null}
                startAtLogin={loginItemOffered && startAtLogin ? { name: startAtLogin.name, enabled: startAtLogin.enabled, toggle: toggleStartAtLogin } : undefined} />
            ) : null}
          </div>
        )}
      </div>
      <footer>
        <section aria-label={pickerText.libraryRegion} className="library-bar">
          <LibraryStatusLine library={library} groupName={groupName} />
          <small className="enter-hints" aria-label={keyText.footer} role="group">
            {altHeld ? <>
              {shiftEnterAction && <span><kbd>{label(keysFor("add"))}</kbd> {shiftEnterAction}</span>}
              <span><kbd>{label(keysFor("escape"))}</kbd> {escapeAction}</span>
            </> : <>
              {enterAction && <span><kbd>{label(keysFor("copy"))}</kbd> {enterAction}</span>}
              <span><kbd>{label(keysFor("keys"))}</kbd> {keyText.allKeys}</span>
            </>}
          </small>
          <span className="more">
            <button
              type="button"
              ref={moreButton}
              className="quiet-button"
              aria-haspopup="menu"
              aria-keyshortcuts={ariaKeys(keysFor("more"), isMac)}
              aria-expanded={menuOpen}
              aria-controls={menuOpen ? "more-menu" : undefined}
              title={pickerText.moreTitle(label(keysFor("more")))}
              onClick={() => (menuOpen ? closeMenu(true) : setMenuOpen(true))}
            >
              {pickerText.more} <kbd aria-hidden="true">{label(keysFor("more"))}</kbd>
            </button>
            {menuOpen && (
              <MoreMenu onClose={closeMenu}>
                {/* Where the references come from and how fresh they are, read
                    as the group's name when the menu is reached by keyboard. */}
                <div role="group" className="menu-library" aria-label={libraryDetail?.filter(Boolean).join(". ")}>
                {libraryDetail && <LibraryDetail lines={libraryDetail} />}
                <MenuItem label={pickerText.chooseLibrary} title={pickerText.chooseTitle} onSelect={() => runFromMenu("choose", chooseLibrary)} />
                {!library?.zotero && (
                  <MenuItem label={pickerText.useZotero} title={pickerText.useZoteroTitle} onSelect={() => runFromMenu("zotero", useZotero)} />
                )}
                {library?.zotero && useZoteroLibraries && (
                  <div role="group" aria-label={pickerText.zoteroLibraries}>
                    {(["user" as const, ...offeredGroups.map((id) => ({ group: id }))] as ZoteroLibrary[]).map((choice) => {
                      const checked = isChosen(choice);
                      const last = checked && chosenLibraries.length === 1;
                      return (
                        <MenuItem key={typeof choice === "object" ? choice.group : choice}
                          label={typeof choice === "object" ? groupName(choice.group) : pickerText.myLibrary}
                          title={last ? pickerText.keepOneLibrary : undefined} checked={checked} disabled={last}
                          onSelect={() => toggleLibrary(choice)} />
                      );
                    })}
                  </div>
                )}
                {library?.name && (
                  <MenuItem label={pickerText.reload} title={pickerText.reloadTitle} onSelect={() => runFromMenu("reload", reloadLibrary)} />
                )}
                </div>
                <MenuItem label={pickerText.density(densityNames[density])} title={pickerText.densityTitle} onSelect={changeDensity} />
                <MenuItem label={pickerText.largerText} checked={largerText} onSelect={toggleLargerText} />
                {loginItemOffered && startAtLogin && (
                  <MenuItem label={loginItemText.label(startAtLogin.name)} title={loginItemText.title} checked={startAtLogin.enabled} onSelect={toggleStartAtLogin} />
                )}
                {/* Closing the window only hides it; this ends the app and frees the shortcut. */}
                <MenuItem label={keyText.title} keys={label(keysFor("keys"))} onSelect={openKeys} />
                <MenuItem label={pickerText.quitLabel} keys={label(keysFor("quit"))} onSelect={() => runFromMenu("quit", quitAfterSaving)} />
              </MoreMenu>
            )}
          </span>
          {library?.note && <p role="status">{library.note}</p>}
        </section>
      </footer>
      {keysOpen && <KeysDialog bindings={bindings} isMac={isMac} onClose={closeKeys} />}
    </main>
  );
}
