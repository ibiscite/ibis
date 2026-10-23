import * as nativeApi from "./api";
import { copiedNoticeText, keyText, libraryMessages, operationFailures as FAILURE_MESSAGES, pickerText } from "./messages";
import { KEY_TABLE, shortcutLabel, type KeyBinding } from "./keys";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { LibrarySetup } from "./LibrarySetup";
import { zoteroLibraryNames } from "./LibraryStatusLine";
import { usePointerSelection } from "./pointerSelection";
import { formatCitation, type FormattedCitation } from "./format/formatted";
import { afterCopy } from "./copyFeedback";
import { KeysDialog } from "./features/picker/KeysDialog";
import { isMac } from "./platform";
import { choiceKey, choiceName, idKey, sameLibrary, styleOf, type Choice } from "./features/picker/choice";
import { composing, isKeysShortcut, isMenuShortcut, isPinShortcut, isPreviewShortcut, isQuitShortcut } from "./features/picker/shortcuts";
import { PickerHeader } from "./features/picker/PickerHeader";
import { Notices, type Operation } from "./features/picker/Notices";
import { CitationShelf } from "./features/picker/CitationShelf";
import { ResultList } from "./features/picker/ResultList";
import { EmptyState } from "./features/picker/EmptyState";
import { PickerFooter } from "./features/picker/PickerFooter";
import { PickerMenu } from "./features/picker/PickerMenu";
import { useOperations } from "./features/picker/useOperations";
import { usePickerPreferences } from "./features/picker/usePickerPreferences";
import {
  type Copied,
  type CopyFormat,
  type LibraryStatus,
  type LoginItem,
  type SearchResult,
  type ShortcutStatus,
  type ZoteroGroup,
  type ZoteroLibrary,
} from "./api";

// A search comes back if the picker returns within a minute, as in Raycast.
const KEEP_SEARCH_MS = 60_000;

const noLibrary = () => null;

// A stable provider lets the development gallery use synthetic data without IPC.
// Formatted citations are offered only when the provider can supply them.
export type PickerApi = Pick<typeof nativeApi,
  "chooseLibrary" | "useZotero" | "libraryStatus" | "reloadLibrary" | "copyCitation" | "copyFormat" | "pickerPreferences" | "setPickerPreferences" |
  "hidePicker" | "setPinned" | "onLibraryChanged" | "onPickerShown" | "onPickerHidden" | "quit" | "search" | "shortcutStatus"> &
  Partial<Pick<typeof nativeApi, "cslItems" | "copyFormatted" | "citationText" | "useZoteroLibraries" | "zoteroGroups" | "findSources" | "openFoundLibrary" | "showCopiedNotice" | "loginItem" | "setLoginItem" | "onUpdateProgress">>;

export type PickerPreview = { query?: string; copied?: Copied };

// The picker: search, choose one or more references, copy them as a citation.
export function App({ api = nativeApi, preview }: { api?: PickerApi; preview?: PickerPreview } = {}) {
  const { chooseLibrary, useZotero, libraryStatus, reloadLibrary, copyCitation,
    hidePicker, setPinned, onLibraryChanged, onPickerShown, onPickerHidden, search, shortcutStatus, cslItems, copyFormatted, citationText,
    useZoteroLibraries, zoteroGroups } = api;
  const { findSources, openFoundLibrary, showCopiedNotice, loginItem, setLoginItem } = api;
  const [updateProgress, setUpdateProgress] = useState<nativeApi.UpdateProgress | null>(null);
  useEffect(() => {
    let mounted = true;
    const subscription = api.onUpdateProgress?.((next) => {
      if (mounted) setUpdateProgress((old) => nativeApi.newerUpdate(old, next));
    }).catch(() => { console.error("Update progress is unavailable."); return null; });
    return () => {
      mounted = false;
      void subscription?.then((stop) => stop?.()).catch(() => console.error("Update event cleanup failed."));
    };
  }, [api]);
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
  const { resetPointer } = pointer;
  useEffect(resetPointer, [results, resetPointer]);
  const [selected, setSelected] = useState<Map<string, SearchResult>>(new Map());
  const [selectionChanged, setSelectionChanged] = useState(false);
  const { failures, setFailures, report, perform } = useOperations();
  const preferences = usePickerPreferences(api, { styles: Boolean(cslItems && copyFormatted), report, perform });
  const { format, density, largerText, showPastePreview, togglePastePreview, changeDensity, toggleLargerText, quitAfterSaving } = preferences;
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
  const output = useRef<HTMLElement | null>(null);
  const outputRef = useCallback((node: HTMLElement | null) => {
    // Saved choices can hide output too. Hand off before removing its focus,
    // without taking focus from More, chips or another control.
    if (!node && output.current?.contains(document.activeElement) && input.current?.isConnected) input.current.focus({ preventScroll: true });
    output.current = node;
  }, []);
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
  // Each completed copy owns only the interaction that started it. Map identity
  // also distinguishes removing and re-adding the same selected references.
  const snapshotRevision = library?.revision;
  const highlightedChoice = choiceKey(results.slice(active, active + 1));
  const latestHighlight = useRef(highlightedChoice);
  latestHighlight.current = highlightedChoice;
  // A selected citation owns its references independently of result highlighting.
  const activeChoice = selected.size === 0 ? highlightedChoice : null;
  const copyContext = useRef({ selected, query, format, snapshotRevision, activeChoice });
  const before = copyContext.current;
  if (before.selected !== selected || before.query !== query || before.format !== format || before.snapshotRevision !== snapshotRevision || before.activeChoice !== activeChoice) {
    copyContext.current = { selected, query, format, snapshotRevision, activeChoice };
  }
  const copyRequest = useRef(0);
  const copyVisibility = useRef(0);
  const pickerShows = useRef(0);
  const currentPin = useRef(pinned);
  currentPin.current = pinned;
  const confirmedPin = useRef(false);
  const pinRequest = useRef(0);
  const pinWork = useRef(Promise.resolve());
  // Setup failures belong to the setup screen; they go when a library is chosen.
  const setupDone = library !== undefined && library?.state !== "none";
  useEffect(() => {
    if (setupDone) setFailures(({ findSources: _found, openFound: _opened, ...rest }) => rest);
  }, [setupDone]);
  // A click still waiting for its preview must never copy once the user has left.
  const [clickCopy, setClickCopy] = useState<string | null>(null);
  const hide = () => {
    copyVisibility.current += 1;
    setClickCopy(null);
    void perform("hide", hidePicker);
  };
  const clearSearch = () => {
    escapedQuery.current = null;
    setQuery("");
    setSelected(new Map());
    setActive(0);
  };
  const chooseFormat = (choice: Choice) => {
    // A new format choice cancels the earlier click waiting for its preview.
    setClickCopy(null);
    preferences.chooseFormat(choice);
  };

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
    const on = !currentPin.current;
    currentPin.current = on;
    setPinnedState(on);
    const request = ++pinRequest.current;
    // Preserve native request order, and roll back only the latest request to
    // the last confirmed state. An older failure cannot undo a newer choice.
    pinWork.current = pinWork.current.then(() => perform("pin", async () => {
      try { await setPinned(on); confirmedPin.current = on; }
      catch (error: unknown) {
        if (request === pinRequest.current) {
          currentPin.current = confirmedPin.current;
          setPinnedState(confirmedPin.current);
        }
        throw error;
      }
    }));
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
      resetPointer();
      copyVisibility.current += 1;
      pickerShows.current += 1;
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
      resetPointer();
      copyVisibility.current += 1;
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
    preferences.loadPreferences(() => mounted);
    return () => {
      acceptInitialStatus = false;
      mounted = false;
      copyVisibility.current += 1;
      void unlistenHidden.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
      void unlisten.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
      void unlistenLibrary.then((stop) => stop()).catch(() => console.error("Library event cleanup failed."));
    };
  }, [resetPointer]);

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
          // An unchanged reply must not undo the user's newer row choice.
          const sameResults = resultsQuery === query && resultsRevision === revision && choiceKey(results) === choiceKey(found);
          if (!sameResults) setActive(0);
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
  // Selection/preview reflow follows the highlight only while Search has focus.
  const previousActive = useRef(active);
  const followKeyboard = useRef(true);
  const resultArea = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const moved = previousActive.current !== active;
    previousActive.current = active;
    if (pointer.fromPointer.current) {
      followKeyboard.current = false;
      pointer.fromPointer.current = false;
      return;
    }
    if (moved) followKeyboard.current = true;
    if (!moved && (selected.size === 0 || document.activeElement !== input.current)) return;
    document.getElementById(`result-${active}`)?.scrollIntoView?.({ block: "nearest" });
  }, [active, pointer.fromPointer, selected.size,
    selected.size > 0 ? formatted : null, selected.size > 0 ? format : null, selected.size > 0 ? searchFocused : null]);

  // Changes below the list can resize its viewport without moving the index.
  // Preserve only Search's keyboard highlight, using the local scroll owners.
  useEffect(() => {
    const area = resultArea.current;
    if (!area) return;
    const scrollers = [area, area.parentElement].filter((node): node is HTMLElement => node !== null);
    const reveal = () => {
      if (!followKeyboard.current || document.activeElement !== input.current) return;
      const row = document.getElementById(`result-${active}`);
      if (!row) return;
      for (const scroller of scrollers) {
        if (scroller.clientHeight <= 0 || !/auto|scroll/.test(getComputedStyle(scroller).overflowY)) continue;
        const top = scroller.getBoundingClientRect().top + scroller.clientTop;
        const bottom = top + scroller.clientHeight;
        const item = row.getBoundingClientRect();
        // A tall row already filling the scrollport cannot be shown any fuller.
        if (item.top <= top && item.bottom >= bottom) continue;
        if (item.bottom > bottom) scroller.scrollTop += Math.ceil(item.bottom - bottom);
        else if (item.top < top) scroller.scrollTop += Math.floor(item.top - top);
      }
    };
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(reveal);
    for (const scroller of scrollers) observer?.observe(scroller);
    window.addEventListener("resize", reveal);
    return () => { observer?.disconnect(); window.removeEventListener("resize", reveal); };
  }, [active]);

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
  // A key syntax is previewed by the backend without touching the clipboard.
  const keyFormat = styleOf(format) === null && citationText ? (format as CopyFormat) : null;
  // A hidden key preview is never fetched: key copying asks the backend itself.
  const keyPreview = showPastePreview ? keyFormat : null;
  const previewKey = `${style ?? keyPreview}\u0002${libraryVersion}\u0002${chosenKey}`;
  // What Enter would copy now; null while it is being formatted.
  const shown = staleSelection ? "stale" : formatted?.key === previewKey ? formatted.result : null;
  // Visibility changes presentation only. Key copying also survives a failed preview.
  const displayedOutput = showPastePreview && chosen.length > 0 && (style || (keyFormat && shown !== "failed")) ? shown : undefined;
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
    if (chosen.length === 0 || staleSelection || !chosenRevision) return;
    let live = true;
    const key = previewKey;
    const replaced = new AbortController();
    const ids = chosen.map((r) => r.id);
    let made: Promise<FormattedCitation>;
    if (style && cslItems) made = cslItems(ids, chosenRevision).then((csl) => formatCitation(csl, style, { signal: replaced.signal }));
    else if (keyPreview && citationText) made = citationText(ids, keyPreview, chosenRevision).then(({ text, warnings }) => ({ markup: [], text, html: "", warnings }));
    else return;
    made
      .then((result) => { if (live) setFormatted({ key, result }); })
      .catch((error) => { if (live) {
        const stale = nativeApi.isSnapshotChanged(error);
        setFormatted({ key, result: stale ? "stale" : "failed" });
        if (stale) refreshSnapshot();
      } });
    return () => { live = false; replaced.abort(); };
  }, [previewKey]);

  // Copy the chosen references in the chosen format.
  const copy = async () => {
    if (chosen.length === 0) return;
    const request = ++copyRequest.current;
    const context = copyContext.current;
    const visibility = copyVisibility.current;
    const shownAt = pickerShows.current;
    const ownsCompletion = () => request === copyRequest.current && context === copyContext.current && visibility === copyVisibility.current;
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
      if (ownsCompletion()) setCopyError(stale ? pickerText.snapshotChanged : pickerText.copyError);
      // A retired request can reveal a replacement after the user has moved on.
      if (stale) refreshSnapshot();
      return;
    }
    if (!ownsCompletion()) return;
    setCopied(outcome);
    const clearedSelection = new Map<string, SearchResult>();
    const completedContext = { ...context, selected: clearedSelection, activeChoice: latestHighlight.current };
    copyContext.current = completedContext;
    setSelected(clearedSelection);
    // After a copy, reopening keeps the current search, not one cleared before it.
    escapedQuery.current = null;
    // Get out of the way so the user can paste, unless there is something to
    // read or the picker is pinned beside the document.
    if (afterCopy(outcome, currentPin.current) === "hide-and-notify") {
      void perform("hide", async () => {
        await hidePicker();
        if (request !== copyRequest.current || shownAt !== pickerShows.current || completedContext !== copyContext.current) return;
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

  const busy = library?.state === "loading" || library?.state === "reloading";

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
  const openMore = () => {
    // React Aria records the invoker before mounting its modal focus scope.
    moreButton.current?.focus();
    setMenuOpen(true);
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
  const enterAction = chosen.length === 0 || shown === "stale" || (style && (shown === null || shown === "failed")) ? null
    : chosen.length > 1 ? keyText.copySeveralAs(chosen.length, choiceName(format)) : keyText.copyAs(choiceName(format));
  const highlighted = current[active];
  const shiftEnterAction = highlighted
    ? (selected.has(idKey(highlighted.id)) ? keyText.removeFromCitation : keyText.addToCitation) : null;
  const escapeAction = menuOpen ? keyText.closeMenu : query ? keyText.clearSearch : keyText.hide;
  const escapeStep = () => {
    setClickCopy(null);
    // Escape steps back one layer, retaining the chosen references.
    if (menuOpen) closeMenu(true);
    else if (query) {
      escapedQuery.current = query;
      setQuery("");
      input.current?.focus();
    } else hide();
  };
  const toggleHighlighted = () => {
    if (highlighted) toggle(highlighted);
    input.current?.focus();
  };
  const emptyState = library !== undefined && (busy || results.length === 0 && !error && !library?.error) ? (
    <EmptyState library={library} query={query} narrowed={narrowed}
      onChooseLibrary={() => void perform("choose", chooseLibrary)}
      onClearSearch={() => { setQuery(""); input.current?.focus(); }}
      onSearchAll={searchAllLibraries}
      onReload={() => { input.current?.focus(); void perform("reload", reloadLibrary); }}
      setup={
        <LibrarySetup api={setupApi} perform={perform}
          writing={{ format, formatted: Boolean(cslItems && copyFormatted), choose: chooseFormat }}
          shortcut={shortcut?.registered ? shortcutLabel(shortcut.shortcut, isMac) : null}
          startAtLogin={loginItemOffered && startAtLogin ? { name: startAtLogin.name, enabled: startAtLogin.enabled, toggle: toggleStartAtLogin } : undefined} />
      } />
  ) : null;

  return (
    <main
      data-selection={selected.size > 0 || undefined}
      onFocus={(e) => setSearchFocused(e.target === input.current)}
      onBlur={() => setSearchFocused(false)}
      onKeyDownCapture={pointer.resetHover}
      onScrollCapture={pointer.resetHover}
      onKeyDown={(e) => {
        if (composing(e)) return;
        if (isPinShortcut(e)) {
          e.preventDefault();
          if (!e.repeat) togglePin();
        } else if (isPreviewShortcut(e)) {
          e.preventDefault();
          if (!e.repeat) togglePastePreview();
        } else if (isKeysShortcut(e)) {
          e.preventDefault();
          openKeys();
        } else if (e.key === "Escape") {
          e.preventDefault();
          escapeStep();
        } else if (isMenuShortcut(e)) {
          e.preventDefault();
          if (menuOpen) closeMenu(true);
          else openMore();
        } else if (isQuitShortcut(e)) {
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
      <PickerHeader
        pinned={pinned} onTogglePin={() => { togglePin(); input.current?.focus(); }}
        inputRef={input}
        query={query}
        onQueryChange={(value) => {
          pointer.resetHover();
          escapedQuery.current = null;
          setQuery(value);
        }}
        onSearchKeyDown={(e) => {
          if (composing(e)) return;
          // A held Remove activation must not act again after focus returns.
          if (e.key === "Enter" && e.repeat) {
            e.preventDefault();
            return;
          }
          if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            followKeyboard.current = true;
            const step = e.key === "ArrowDown" ? 1 : -1;
            setActive((i) => Math.min(Math.max(i + step, 0), Math.max(results.length - 1, 0)));
          } else if (e.key === "Enter" && e.shiftKey) {
            e.preventDefault();
            toggleHighlighted();
          } else if (e.key === "Enter") {
            e.preventDefault();
            void copy();
          }
        }}
        expanded={results.length > 0}
        activeDescendant={results[active] ? `result-${active}` : undefined}
        format={format}
        showFormat={library === null || Boolean(library?.has_snapshot)}
        styles={Boolean(cslItems && copyFormatted)}
        onFormatChange={chooseFormat}
        onFormatCommand={(command) => {
          const visibility = copyVisibility.current;
          return {
            isCurrent: () => visibility === copyVisibility.current && hiddenAt.current === null,
            run: () => {
              if (command === "more") openMore();
              else if (command === "keys") openKeys();
              else if (command === "pin") togglePin();
              else if (command === "preview") togglePastePreview();
              else if (command === "quit") void perform("quit", quitAfterSaving);
            },
          };
        }}
      />
      <div className="picker-content">
      <Notices updateProgress={updateProgress} library={library} failures={failures} startAtLogin={startAtLogin}
        error={error} copyError={copyError} copied={copied}
        showCopiedText={displayedOutput == null || typeof displayedOutput === "string" || displayedOutput.text !== copied?.text}
        onDismissCopied={() => setCopied(null)} />
      <div className="results-area" ref={resultArea}>
        {library?.has_snapshot && results.length > 0 && !busy && <div className="results-heading" aria-hidden="true">
          <span>{query ? pickerText.bestMatches : pickerText.references}</span><bdi>{pickerText.resultCount(results.length)}</bdi>
        </div>}
        <p className="visually-hidden" role="status" aria-label={pickerText.searchResults} aria-live="polite" aria-atomic="true">
          {announcement}
        </p>
        {busy && emptyState}
        <ResultList results={results} active={active} selected={selected} density={density} hoverAllowed={pointer.hoverAllowed}
          libraryOf={severalLibraries ? libraryOf : noLibrary} onMouseMove={pointer.onMouseMove}
          onChoose={(i, r, action) => {
            // Rows of an earlier query, still shown while a newer search
            // is pending, are not acted on by a click or Shift+click (#507).
            if (current !== results) return;
            setActive(i);
            if (action === "toggle") toggle(r);
            else setClickCopy(selected.size > 0 ? chosenKey : choiceKey([r]));
            input.current?.focus();
          }}
          onToggle={(i, r) => {
            if (current !== results) return;
            setActive(i);
            toggle(r);
            input.current?.focus();
          }} />
        {!busy && emptyState}
      </div>
      </div>
      <CitationShelf announce={selectionChanged} selected={selected} removeLabel={removeLabel} onRemove={(r) => {
        toggle(r);
        input.current?.focus({ preventScroll: true });
      }} style={style} keys={keyFormat !== null}
        shown={displayedOutput} focusable={selected.size > 0} outputRef={outputRef} />
      <PickerFooter library={library} groupName={groupName}
        hints={{ enter: enterAction, shiftEnter: shiftEnterAction, escape: escapeAction }}
        altHeld={altHeld} actions={{ copy: () => { void copy(); }, add: toggleHighlighted, escape: escapeStep, keys: openKeys }}
        more={
          <PickerMenu isOpen={menuOpen} onOpenChange={(open) => (open ? openMore() : closeMenu(false))}
            buttonRef={moreButton} searchRef={input} library={library} libraryDetail={libraryDetail}
            zotero={library?.zotero && useZoteroLibraries ? {
              offered: ["user" as const, ...offeredGroups.map((id) => ({ group: id }))],
              isChosen, chosenCount: chosenLibraries.length,
              name: (choice) => (typeof choice === "object" ? groupName(choice.group) : pickerText.myLibrary),
              toggle: toggleLibrary,
            } : null}
            density={density} largerText={largerText} showPastePreview={showPastePreview} pinned={pinned}
            startAtLogin={loginItemOffered && startAtLogin ? { item: startAtLogin, toggle: toggleStartAtLogin } : null}
            actions={{
              choose: () => runFromMenu("choose", chooseLibrary),
              useZotero: () => runFromMenu("zotero", useZotero),
              reload: () => runFromMenu("reload", reloadLibrary),
              density: changeDensity,
              largerText: toggleLargerText,
              preview: togglePastePreview,
              pin: togglePin,
              keys: openKeys,
              quit: () => runFromMenu("quit", quitAfterSaving),
            }} />
        } />
      {keysOpen && <KeysDialog bindings={bindings} isMac={isMac} onClose={closeKeys} />}
    </main>
  );
}
