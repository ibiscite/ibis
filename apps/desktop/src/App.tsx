import * as nativeApi from "./api";
import { copiedNoticeText, keyText, libraryMessages, operationFailures as FAILURE_MESSAGES, pickerText } from "./messages";
import { KEY_TABLE, type KeyBinding } from "./keys";
import { useEffect, useMemo, useRef, useState } from "react";
import { LibrarySetup } from "./LibrarySetup";
import { focusChoices } from "./ui/ChoiceList";
import { zoteroLibraryNames } from "./LibraryStatusLine";
import { usePointerSelection } from "./pointerSelection";
import { afterCopy, noticeTitle } from "./copyFeedback";
import { Button } from "./ui/Button";
import { KeysDialog } from "./features/picker/KeysDialog";
import { isMac } from "./platform";
import { choiceKey, idKey, styleOf, type Choice, type CitationReference } from "./features/picker/choice";
import { enterLabel } from "./features/picker/enterLabel";
import { useDeliveryTarget } from "./features/picker/useDeliveryTarget";
import { composing, isFormatShortcut, isKeysShortcut, isMenuShortcut, isPinShortcut, isPreviewShortcut, isQuitShortcut, isSettingsShortcut } from "./features/picker/shortcuts";
import { PickerHeader } from "./features/picker/PickerHeader";
import { Notices, type Operation } from "./features/picker/Notices";
import { CitationShelf } from "./features/picker/CitationShelf";
import { ResultList } from "./features/picker/ResultList";
import { closestMatchWarnings } from "./features/picker/rows";
import { EmptyState } from "./features/picker/EmptyState";
import { useFitWindow } from "./features/picker/useFitWindow";
import { PickerFooter } from "./features/picker/PickerFooter";
import { PickerMenu } from "./features/picker/PickerMenu";
import { useOperations } from "./features/picker/useOperations";
import { usePickerPreferences } from "./features/picker/usePickerPreferences";
import { useLibrarySearch } from "./features/picker/useLibrarySearch";
import { useCitationOutput } from "./features/picker/useCitationOutput";
import { useZoteroScope } from "./features/picker/useZoteroScope";
import { useDestination } from "./features/picker/useDestination";
import { useLoginItem } from "./features/picker/useLoginItem";
import { useUpdateChecks } from "./features/picker/useUpdateChecks";
import { useShortcutStatus } from "./features/picker/useShortcutStatus";
import { usePin } from "./features/picker/usePin";
import {
  type Copied,
  type LibraryStatus,
  type Locator,
  type SearchResult,
} from "./api";

// A search comes back if the picker returns within a minute, as in Raycast.
const KEEP_SEARCH_MS = 60_000;

const noLibrary = () => null;
const noSelection = new Map<string, CitationReference>();

// A stable provider lets component tests use synthetic data without IPC.
// Formatted citations are offered only when the provider can supply them.
export type PickerApi = Pick<typeof nativeApi,
  "chooseLibrary" | "useZotero" | "libraryStatus" | "reloadLibrary" | "previewFileCitation" | "copyFileCitation" | "copyFormat" | "pickerPreferences" | "setPickerPreferences" |
  "hidePicker" | "pickerVisible" | "setPinned" | "onLibraryChanged" | "onPickerShown" | "onPickerHidden" | "quit" | "search" | "shortcutStatus"> &
  Partial<Pick<typeof nativeApi, "useZoteroLibraries" | "zoteroGroups" | "pickerDestination" | "rememberAppOutput" | "forgetAppOutputs" | "fitPicker" | "findSources" | "openFoundLibrary" | "rowContext" | "showCopiedNotice" | "loginItem" | "setLoginItem" | "onUpdateProgress" | "updateChecks" | "setUpdateChecks" | "onUpdateChecks" | "updateAvailable" | "onUpdateAvailable" | "checkForUpdates" | "onShortcutStatus" | "openSettings" | "onPreferencesChanged">>;

export type PickerPreview = { query?: string; copied?: Copied };

// The picker: search, choose one or more references, copy them as a citation.
export function App({ api = nativeApi, preview }: { api?: PickerApi; preview?: PickerPreview } = {}) {
  const { chooseLibrary, useZotero, libraryStatus, reloadLibrary,
    hidePicker, pickerVisible, setPinned, onLibraryChanged, onPickerShown, onPickerHidden, search, shortcutStatus, previewFileCitation, copyFileCitation,
    useZoteroLibraries: chooseZoteroLibraries, zoteroGroups } = api;
  const { rowContext, findSources, openFoundLibrary, showCopiedNotice, loginItem, setLoginItem, fitPicker, updateChecks, setUpdateChecks, onUpdateChecks, updateAvailable, onUpdateAvailable, checkForUpdates, onShortcutStatus, openSettings, onPreferencesChanged } = api;
  const [updateProgress, setUpdateProgress] = useState<nativeApi.UpdateProgress | null>(null);
  const setupApi = useMemo(() => ({ chooseLibrary, useZotero, findSources, openFoundLibrary }),
    [chooseLibrary, useZotero, findSources, openFoundLibrary]);
  const [query, setQuery] = useState(preview?.query ?? "");
  const [interpretLocators, setInterpretLocators] = useState(true);
  const [active, setActive] = useState(0);
  const pointer = usePointerSelection(active, setActive);
  const { resetPointer } = pointer;
  const [selected, setSelected] = useState<Map<string, CitationReference>>(new Map());
  const [selectionChanged, setSelectionChanged] = useState(false);
  const { failures, setFailures, report, perform } = useOperations();
  const { shortcut, readShortcut } = useShortcutStatus({ shortcutStatus, onShortcutStatus, report });
  useEffect(() => {
    let mounted = true;
    setFailures((old) => old.updateEvents ? { ...old, updateEvents: false } : old);
    const subscription = api.onUpdateProgress?.((next) => {
      if (mounted) setUpdateProgress((old) => nativeApi.newerUpdate(old, next));
    }).catch(() => {
      if (mounted) setFailures((old) => ({ ...old, updateEvents: true }));
      return null;
    });
    return () => {
      mounted = false;
      void subscription?.then((stop) => stop?.()).catch(() => console.error("Update event cleanup failed."));
    };
  }, [api, setFailures]);
  const preferences = usePickerPreferences(api, { styles: true, report, perform });
  const { density, largerText, searchMark, showPastePreview, togglePastePreview, changeDensity, toggleLargerText, toggleSearchMark, quitAfterSaving: quitPreferences } = preferences;
  const deliveryTarget = useDeliveryTarget(api);
  const destination = useDestination(deliveryTarget.api, { fallback: preferences.format, fallbackReady: preferences.available, chooseDefault: preferences.chooseFormat, report });
  const { format } = destination;
  const quitAfterSaving = () => destination.quitAfterSaving(quitPreferences);
  const style = styleOf(format);
  const [copied, setCopied] = useState<Copied | null>(preview?.copied ?? null);
  const [copyError, setCopyError] = useState<string | null>(null);
  // Only the outcome, never citation or document text; kept until the next copy.
  const [lastOutcome, setLastOutcome] = useState<string | null>(null);
  // Changes whenever a library opens, so the current query is searched again.
  const [libraryVersion, setLibraryVersion] = useState(0);
  const statusReads = useRef(0);
  // Undefined while loading; null means the status request itself failed.
  const [library, setLibrary] = useState<LibraryStatus | null>();
  const busy = library?.state === "loading" || library?.state === "reloading";
  const blocked = library?.state === "error" && !library.has_snapshot;
  const libraryChoices = library?.state === "none" || blocked || library?.state === "loaded" && library.has_snapshot && library.references === 0;
  const showCitations = !libraryChoices;
  const refreshSnapshot = () => {
    const request = ++statusReads.current;
    void libraryStatus().then((status) => {
      if (statusReads.current !== request) return;
      setLibrary(status);
      setLibraryVersion((version) => version + 1);
      report("status", false);
    }).catch(() => { if (statusReads.current === request) report("status"); });
  };
  const { queryKey, results, current, interpretation, list, announcement, resultsQuery, error, pending: searchPending } =
    useLibrarySearch({ search, query, interpretLocators, library, libraryVersion, setActive, refreshSnapshot });
  // biome-ignore lint/correctness/useExhaustiveDependencies: new results are the trigger: the pointer must move again before it counts.
  useEffect(resetPointer, [results, resetPointer]);
  // The selection, otherwise the current pasted list or highlighted result.
  const chosen = !showCitations ? [] : selected.size > 0 ? [...selected.values()] : list ? current : current.slice(active, active + 1).map((r) => ({ ...r, locator: interpretation?.locator }));
  const { formatted, chosenKey, chosenRevision, keyFormat, shown, previewProblem, displayedOutput } =
    useCitationOutput({ chosen, library, libraryVersion, format, style, showPastePreview, previewFileCitation, refreshSnapshot });
  const input = useRef<HTMLInputElement>(null);
  const formatButtonRef = useRef<HTMLButtonElement>(null);
  const moreButton = useRef<HTMLButtonElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const { startAtLogin, readLoginItem, toggleStartAtLogin, offered: loginItemOffered } = useLoginItem({ loginItem, setLoginItem, perform, menuOpen });
  const updateMenu = useUpdateChecks({ updateChecks, setUpdateChecks, onUpdateChecks, updateAvailable, onUpdateAvailable, perform, report });
  const { pinned, currentPin, togglePin } = usePin({ setPinned, perform });
  const [keysOpen, setKeysOpen] = useState(false);
  // Where focus goes back to when the keys overlay closes.
  const keysReturn = useRef<HTMLElement | null>(null);
  // While Alt (⌥) is held, the footer shows the other keys instead of Enter.
  const [altHeld, setAltHeld] = useState(false);
  // Enter copies only from the search box; elsewhere it presses the focused control.
  const [searchFocused, setSearchFocused] = useState(false);
  // autoFocus can happen before React listens for focus events.
  useEffect(() => setSearchFocused(document.activeElement === input.current), []);
  // When the picker last went away; the search waits a minute, in memory only.
  const hiddenAt = useRef<number | null>(null);
  // A permission detour owns the prepared shelf, even beyond the usual minute.
  const permissionDetour = useRef(false);
  // A search Escape cleared comes back on reopening within the same minute.
  const escapedQuery = useRef<{ text: string; interpretLocators: boolean } | null>(null);
  const [restored, setRestored] = useState(0);
  // Each completed copy owns only the interaction that started it. Map identity
  // also distinguishes removing and re-adding the same selected references.
  const snapshotRevision = library?.revision;
  const highlightedChoice = choiceKey(results.slice(active, active + 1).map((r) => ({ ...r, locator: interpretation?.locator })));
  const latestHighlight = useRef(highlightedChoice);
  latestHighlight.current = highlightedChoice;
  const defaultChoice = list ? choiceKey(current) : highlightedChoice;
  const latestDefaultChoice = useRef(defaultChoice);
  latestDefaultChoice.current = defaultChoice;
  // A selected citation owns its references independently of result highlighting.
  const activeChoice = selected.size === 0 ? defaultChoice : null;
  const copyContext = useRef({ selected, queryKey, format, snapshotRevision, activeChoice });
  const before = copyContext.current;
  if (before.selected !== selected || before.queryKey !== queryKey || before.format !== format || before.snapshotRevision !== snapshotRevision || before.activeChoice !== activeChoice) {
    copyContext.current = { selected, queryKey, format, snapshotRevision, activeChoice };
  }
  const copyRequest = useRef(0);
  const lastSuccessfulCopy = useRef(0);
  // Query and chooser scope do not change a citation already being delivered.
  const copiesInFlight = useRef(new Set<{ choice: string; format: Choice; visibility: number; opening: number }>());
  const copyVisibility = useRef(0);
  const pickerShows = useRef(0);
  const copyingCurrent = (choice = chosenKey) => {
    const opening = destination.copyToken().opening;
    for (const flight of copiesInFlight.current) {
      if (flight.choice === choice && flight.format === format && flight.visibility === copyVisibility.current && flight.opening === opening) return true;
    }
    return false;
  };
  // Setup failures belong to the setup screen; they go when a library is chosen.
  const setupDone = library !== undefined && library?.state !== "none";
  useEffect(() => {
    if (setupDone) setFailures(({ findSources: _found, openFound: _opened, ...rest }) => rest);
  }, [setupDone, setFailures]);
  // A copy (Enter or a click) still waiting for its preview must never copy once the user has left.
  const [pendingCopy, setPendingCopy] = useState<{ choice: string | null; selection: typeof selected; queryKey: string; format: Choice | null; output: ReturnType<typeof destination.copyToken> } | null>(null);
  const requestCopy = (choice: string | null) => {
    if (!copyingCurrent(choice ?? chosenKey) && hiddenAt.current === null) setPendingCopy({ choice, selection: selected, queryKey,
      format: destination.ready && preferences.ready ? format : null, output: destination.copyToken() });
  };
  const hide = () => {
    copyVisibility.current += 1;
    setPendingCopy(null);
    void perform("hide", hidePicker);
  };
  const clearSearch = () => {
    escapedQuery.current = null;
    setQuery("");
    setInterpretLocators(true);
    setSelected(new Map());
    setActive(0);
  };
  const chooseFormat = (choice: Choice) => {
    // A new format choice cancels the earlier copy waiting for its preview.
    setPendingCopy(null);
    destination.choose(choice);
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

  // When the picker is shown, put the cursor in the search box with the
  // previous query selected, so typing replaces it.
  // biome-ignore lint/correctness/useExhaustiveDependencies: subscribes once on mount. The handlers only call setters, refs and native calls that never change, so the first render's copies are current.
  useEffect(() => {
    let acceptInitialStatus = true;
    let mounted = true;
    let visibilityEvents = 0;
    const subscriptionFailure = (operation: Operation) => {
      if (mounted) report(operation);
      else console.error(FAILURE_MESSAGES[operation]);
      return () => undefined;
    };
    const focusSearch = () => {
      resetPointer();
      copyVisibility.current += 1;
      pickerShows.current += 1;
      if (!permissionDetour.current && hiddenAt.current !== null && performance.now() - hiddenAt.current > KEEP_SEARCH_MS) clearSearch();
      else if (hiddenAt.current !== null && escapedQuery.current !== null) {
        const escaped = escapedQuery.current;
        escapedQuery.current = null;
        setQuery((now) => (now === "" ? escaped.text : now));
        setInterpretLocators(escaped.interpretLocators);
        setRestored((n) => n + 1);
      }
      hiddenAt.current = null;
      destination.read(true, permissionDetour.current);
      readLoginItem();
      readShortcut();
      input.current?.focus();
      input.current?.select();
    };
    // Every hide is reported, including the native one when focus moves away;
    // a monotonic clock keeps clock changes from clearing or keeping a search.
    const unlistenHidden = onPickerHidden(() => {
      visibilityEvents += 1;
      // A hidden native window may not throttle the web view: pause looping animations.
      document.documentElement.dataset.pickerHidden = "";
      resetPointer();
      copyVisibility.current += 1;
      hiddenAt.current = performance.now();
      setKeysOpen(false);
      setAltHeld(false);
      setPendingCopy(null);
    })
      .catch(() => subscriptionFailure("pickerEvents"));
    const unlisten = onPickerShown(() => {
      visibilityEvents += 1;
      delete document.documentElement.dataset.pickerHidden;
      focusSearch();
    }).catch(() => subscriptionFailure("pickerEvents"));
    void Promise.all([unlistenHidden, unlisten]).then(async () => {
      if (!mounted) return;
      const events = visibilityEvents;
      const visible = await pickerVisible();
      // A newer event wins over this read; failed reads leave the flag alone.
      if (!mounted || events !== visibilityEvents) return;
      if (visible) delete document.documentElement.dataset.pickerHidden;
      else document.documentElement.dataset.pickerHidden = "";
    }).catch(() => undefined);
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
    destination.read();
    preferences.loadPreferences(() => mounted);
    // Choices made in Settings apply to the picker at once.
    const unlistenPreferences = onPreferencesChanged?.(() => { preferences.loadPreferences(() => mounted); destination.read(false); })
      .catch(() => undefined);
    return () => {
      acceptInitialStatus = false;
      mounted = false;
      copyVisibility.current += 1;
      void unlistenHidden.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
      void unlisten.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
      void unlistenLibrary.then((stop) => stop()).catch(() => console.error("Library event cleanup failed."));
      void unlistenPreferences?.then((stop) => stop?.());
    };
  }, [resetPointer]);

  // A restored search is selected once it is back in the box, so typing replaces it.
  useEffect(() => {
    if (restored > 0) input.current?.select();
  }, [restored]);

  // Keep the highlighted result visible while moving with the arrow keys.
  // A row clicked is already under the pointer; scrolling would move it away.
  // Selection/preview reflow follows the highlight only while Search has focus.
  const previousActive = useRef(active);
  const followKeyboard = useRef(true);
  const resultArea = useRef<HTMLDivElement>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the conditional values are triggers: while references are chosen, a preview, format or focus change can move the row, so it is revealed again.
  useEffect(() => {
    const moved = previousActive.current !== active;
    previousActive.current = active;
    const from = pointer.cameFrom(active);
    if (from !== "other") {
      if (from === "click") followKeyboard.current = false;
      return;
    }
    if (moved) followKeyboard.current = true;
    if (!moved && (selected.size === 0 || document.activeElement !== input.current)) return;
    document.getElementById(`result-${active}`)?.scrollIntoView?.({ block: "nearest" });
  }, [active, pointer.cameFrom, selected.size,
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

  const toggle = (result?: SearchResult) => {
    setPendingCopy(null);
    if (!result) return;
    setSelectionChanged(true);
    setSelected((previous) => {
      const next = new Map(previous);
      if (!next.delete(idKey(result.id))) next.set(idKey(result.id), { ...result, locator: interpretation?.locator ? { ...interpretation.locator } : null });
      return next;
    });
  };

  const changeLocator = (reference: CitationReference, locator: Readonly<Locator> | null) => {
    setPendingCopy(null);
    setSelected((previous) => {
      if (previous.get(idKey(reference.id)) !== reference) return previous;
      const next = new Map(previous);
      next.set(idKey(reference.id), { ...reference, locator: locator ? { ...locator } : null });
      return next;
    });
    input.current?.focus({ preventScroll: true });
  };

  const changeReference = (reference: CitationReference, change: Pick<CitationReference, "locator" | "form">) => {
    setPendingCopy(null);
    setSelectionChanged(true);
    setSelected((previous) => {
      const prepared = previous.get(idKey(reference.id));
      if (prepared ? prepared !== reference : previous.size > 0) return previous;
      const next = new Map(previous);
      next.set(idKey(reference.id), { ...reference, ...change });
      return next;
    });
  };

  const rowScope = useMemo(() => ({ api, current, selected, queryKey, format, snapshotRevision }), [api, current, selected, queryKey, format, snapshotRevision]);
  const latestRowScope = useRef(rowScope);
  latestRowScope.current = rowScope;
  const contextRequest = useRef(0);

  // Copy the chosen references in the chosen format.
  const executeCopy = async () => {
    if (chosen.length === 0) return;
    const request = ++copyRequest.current;
    const context = copyContext.current;
    const outputToken = destination.copyToken();
    const visibility = copyVisibility.current;
    const shownAt = pickerShows.current;
    const ownsCompletion = () => request === copyRequest.current && context === copyContext.current && visibility === copyVisibility.current && destination.isCurrentCopy(outputToken);
    setCopied(null);
    setCopyError(null);
    // The shelf already shows the actionable refusal, even with its text hidden.
    if (shown === "failed") return;
    if (shown === "stale" || !chosenRevision) {
      setCopyError(pickerText.snapshotChanged);
      refreshSnapshot();
      return;
    }
    let outcome: Copied;
    try {
      if (!shown || typeof shown === "string" || !shown.fileTicket || !shown.fileHash) {
        setCopyError(shown === null ? pickerText.previewNotReady : pickerText.previewError);
        return;
      }
      outcome = await copyFileCitation(shown.fileTicket, shown.fileHash);
      outcome = { ...outcome, warnings: [...outcome.warnings, ...closestMatchWarnings(chosen)] };
    } catch (error) {
      const stale = nativeApi.isSnapshotChanged(error);
      const shared = nativeApi.sharedCitationKey(error);
      if (ownsCompletion()) setCopyError(stale ? pickerText.snapshotChanged : shared ? pickerText.sharedKey(shared) : pickerText.copyError);
      // A retired request can reveal a replacement after the user has moved on.
      if (stale) refreshSnapshot();
      return;
    }
    // Failed or unfinished newer attempts do not retire this copy's receipt.
    if (request > lastSuccessfulCopy.current) {
      lastSuccessfulCopy.current = request;
      setLastOutcome(copiedNoticeText.title);
    }
    const ownsFeedback = ownsCompletion();
    if (!ownsFeedback) { destination.consumeOnce(outputToken); return; }
    setCopied(outcome);
    const complete = () => {
      const nextFormat = destination.consumeOnce(outputToken);
      const clearedSelection = new Map<string, CitationReference>();
      copyContext.current = { ...context, format: nextFormat ?? context.format, selected: clearedSelection, activeChoice: latestDefaultChoice.current };
      setSelected(clearedSelection);
      permissionDetour.current = false;
    };
    // After a copy, reopening keeps the current search, not one cleared before it.
    escapedQuery.current = null;
    // Get out of the way so the user can paste, unless there is something to
    // read or the picker is pinned beside the document.
    if (afterCopy(outcome, currentPin.current) === "hide-and-notify") {
      // Keep the shelf until delivery says whether macOS sent us to its guide.
      // A return during delivery also keeps it and cancels this completion.
      permissionDetour.current = true;
      await perform("hide", async () => {
        const current = () => request === copyRequest.current && shownAt === pickerShows.current && context === copyContext.current;
        let permissionRefused = false;
        try {
          await hidePicker();
          if (!current()) return;
          // The notice is extra; a failure to show it never undoes the copy.
          const delivered = await showCopiedNotice?.(outcome.text, outcome.delivery ?? null).catch(() => console.error(copiedNoticeText.noticeFailed));
          if (delivered && request === lastSuccessfulCopy.current) setLastOutcome(noticeTitle({ delivered, text: "" }));
          permissionRefused = !!delivered && delivered.outcome === "copied" && delivered.reason === "no-permission";
        } finally {
          if (current() && !permissionRefused) complete();
        }
      });
    } else complete();
  };
  const copy = async () => {
    if (copyingCurrent()) return;
    const flight = { choice: chosenKey, format, visibility: copyVisibility.current, opening: destination.copyToken().opening };
    copiesInFlight.current.add(flight);
    try { await executeCopy(); }
    finally { copiesInFlight.current.delete(flight); }
  };
  // Enter, Copy or a click on a row copies what is chosen then (the row, or the
  // citation being built), once its preview is ready; anything changing first cancels it.
  const copyChosen = () => {
    if (chosen.length > 0) requestCopy(chosenKey);
    else if (showCitations && searchPending) requestCopy(null);
  };
  // biome-ignore lint/correctness/useExhaustiveDependencies: copy is rebuilt every render; this runs when the waiting copy, the choice, its preview or the search changes.
  useEffect(() => {
    if (pendingCopy === null) return;
    if (pendingCopy.choice !== null && chosenKey !== pendingCopy.choice || selected !== pendingCopy.selection || queryKey !== pendingCopy.queryKey
      || pendingCopy.format !== null && format !== pendingCopy.format || !destination.isCurrentCopy(pendingCopy.output)) {
      setPendingCopy(null);
      return;
    }
    if (pendingCopy.choice === null) {
      if (searchPending) return;
      setPendingCopy(chosen.length > 0 ? { ...pendingCopy, choice: chosenKey } : null);
      return;
    }
    if (shown === null || !destination.ready || destination.saving || !preferences.ready) return;
    setPendingCopy(null);
    void copy();
  }, [pendingCopy, chosenKey, selected, shown, queryKey, searchPending, format, destination.ready, destination.saving, preferences.ready]);
  // Nothing matched this search, so the empty state offers what to try next.
  const nothingMatched = Boolean(query) && resultsQuery === queryKey && results.length === 0 && !error;
  const { chosenLibraries, severalLibraries, isChosen, toggleLibrary, searchAllLibraries, groupName, offeredGroups, libraryOf, narrowed } =
    useZoteroScope({ library, libraryVersion, menuOpen, nothingMatched, zoteroGroups, chooseZoteroLibraries, report, perform,
      focusSearch: () => input.current?.focus() });
  // Read when More opens, the only time it is shown there.
  const libraryDetail = menuOpen ? libraryMessages.detail(library, Date.now(), (status) => zoteroLibraryNames(status, groupName)) : null;
  /** Names the library too when another chosen reference has the same key. */
  const removeLabel = (r: SearchResult) => {
    const library = libraryOf(r.source ?? "");
    const shared = [...selected.values()].some((other) => other.key === r.key && other.source !== r.source);
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
  // A refused or stale engine preview cannot be copied.
  const enterAction = chosen.length === 0 || shown === "stale" || shown === "failed" ? null
    : enterLabel(format, chosen.length, destination.app, deliveryTarget.canPaste,
      pinned || Boolean(shown && typeof shown === "object" && (shown.warnings.length || closestMatchWarnings(chosen).length)), Boolean(list && selected.size === 0));
  const highlighted = current[active];
  const usingLocator = current === results && interpretLocators && showCitations ? interpretation?.locator : null;
  const prepared = highlighted && selected.get(idKey(highlighted.id));
  const canApplyLocator = usingLocator && prepared && prepared.revision === highlighted.revision &&
    choiceKey([prepared]) !== choiceKey([{ ...highlighted, locator: usingLocator, form: prepared.form }]);
  const shiftEnterAction = highlighted && showCitations
    ? (selected.has(idKey(highlighted.id)) ? keyText.removeFromCitation : keyText.addToCitation) : null;
  const escapeAction = menuOpen ? keyText.closeMenu : query ? keyText.clearSearch : keyText.hide;
  const escapeStep = () => {
    setPendingCopy(null);
    // Escape steps back one layer, retaining the chosen references.
    if (menuOpen) closeMenu(true);
    else if (query) {
      escapedQuery.current = { text: query, interpretLocators };
      setQuery("");
      setInterpretLocators(true);
      input.current?.focus();
    } else hide();
  };
  const toggleHighlighted = () => {
    toggle(highlighted);
    input.current?.focus();
  };
  const emptyState = (library !== undefined && (libraryChoices || busy || results.length === 0 && !error && (!library?.error || blocked)) ? (
    <EmptyState library={library} query={query} narrowed={narrowed}
      onChooseLibrary={() => void perform("choose", chooseLibrary)}
      onClearSearch={() => { setQuery(""); setInterpretLocators(true); input.current?.focus(); }}
      onSearchAll={searchAllLibraries}
      onReload={() => { input.current?.focus(); void perform("reload", reloadLibrary); }}
      setup={
        (focus, empty) => <LibrarySetup api={setupApi} perform={perform} autoFocus={focus} empty={empty} />
      } />
  ) : null);

  // Size follows content (C10): only results, a citation and overlays need the results height.
  const main = useRef<HTMLElement>(null);
  const compact = !keysOpen && !menuOpen && (!showCitations || selected.size === 0 && !(query && results.length > 0));
  useFitWindow(main, compact, fitPicker, fitPicker ? onPickerShown : undefined);

  return (
    <main ref={main}
      data-selection={showCitations && selected.size > 0 || undefined}
      data-compact={compact || undefined}
      onFocus={(e) => setSearchFocused(e.target === input.current)}
      onBlur={() => setSearchFocused(false)}
      onKeyDownCapture={pointer.resetHover}
      onScrollCapture={pointer.resetHover}
      onKeyDown={(e) => {
        if (composing(e)) return;
        if (isFormatShortcut(e)) {
          e.preventDefault();
          if (!e.repeat && !menuOpen && !keysOpen) formatButtonRef.current?.click();
        } else if (isPinShortcut(e)) {
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
        } else if (isSettingsShortcut(e) && openSettings) {
          e.preventDefault();
          if (!e.repeat) runFromMenu("settings", () => openSettings(null));
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
        pinned={pinned} mark={searchMark} onTogglePin={() => { togglePin(); input.current?.focus(); }}
        inputRef={input}
        query={query}
        onQueryChange={(value) => {
          pointer.resetHover();
          escapedQuery.current = null;
          setQuery(value);
          setInterpretLocators(true);
        }}
        onSearchKeyDown={(e) => {
          if (composing(e)) return;
          // A held Remove activation must not act again after focus returns.
          if (e.key === "Enter" && e.repeat) {
            e.preventDefault();
            return;
          }
          if (e.key === "ArrowDown" && (results.length === 0 || !showCitations) && focusChoices()) {
            e.preventDefault();
          } else if (!showCitations && e.key === "Enter") {
            // Choice screens have no citation action in Search.
            e.preventDefault();
          } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            setPendingCopy(null);
            followKeyboard.current = true; pointer.forgetClick();
            const step = e.key === "ArrowDown" ? 1 : -1;
            setActive((i) => Math.min(Math.max(i + step, 0), Math.max(results.length - 1, 0)));
          } else if (e.key === "Enter" && e.shiftKey) {
            e.preventDefault();
            toggleHighlighted();
          } else if (e.key === "Enter") {
            e.preventDefault();
            copyChosen();
          }
        }}
        expanded={results.length > 0 && showCitations}
        activeDescendant={showCitations && results[active] ? `result-${active}` : undefined}
        format={format}
        formatButtonRef={formatButtonRef}
        app={destination.app}
        onceOnly={destination.onceOnly}
        onToggleOnce={destination.toggleOnce}
        onAutomatic={destination.restoreAutomatic}
        onDefault={destination.useDefault}
        formatDisabled={!destination.ready || destination.saving}
        showFormat={!libraryChoices && (library === null || Boolean(library?.has_snapshot))}
        styles
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
        error={error} copyError={copyError} copied={copied} missing={list?.missing}
        showCopiedText={displayedOutput == null || typeof displayedOutput === "string" || displayedOutput.text !== copied?.text}
        onDismissCopied={() => setCopied(null)} />
      {usingLocator && <div className="notices">
        <p role="status">{pickerText.usingLocator(usingLocator.label, usingLocator.value)}{" "}
          <Button variant="quiet" size="small" onPress={() => { setInterpretLocators(false); input.current?.focus({ preventScroll: true }); }}>
            {pickerText.searchAsTyped}
          </Button>
          {canApplyLocator && <Button variant="quiet" size="small" onPress={() => changeLocator(prepared, usingLocator)}>
            {pickerText.applyLocator(prepared.key)}
          </Button>}
        </p>
      </div>}
      <div className="results-area scroll-area" ref={resultArea}>
        {library?.has_snapshot && results.length > 0 && !busy && showCitations && <div className="results-heading" aria-hidden="true">
          <span>{list ? pickerText.pastedList : results.some((r) => r.partial) ? pickerText.closestMatches : query ? pickerText.bestMatches : pickerText.references}</span><bdi>{pickerText.resultCount(results.length)}</bdi>
        </div>}
        <p className="visually-hidden" role="status" aria-label={pickerText.searchResults} aria-live="polite" aria-atomic="true">
          {showCitations ? announcement : ""}
        </p>
        {busy && emptyState}
        {showCitations && <ResultList results={results} active={active} selected={selected} density={density} hoverAllowed={pointer.hoverAllowed}
          libraryOf={severalLibraries ? libraryOf : noLibrary} onMouseMove={pointer.onMouseMove}
          onChoose={(i, r, action) => {
            // Rows of an earlier query, still shown while a newer search
            // is pending, are not acted on by a click or Shift+click (#507).
            if (action === "toggle") setPendingCopy(null);
            if (current !== results) return;
            pointer.choose(i);
            if (action === "toggle") toggle(r);
            else if (action === "copy") requestCopy(selected.size > 0 || list ? chosenKey : choiceKey([{ ...r, locator: interpretation?.locator }]));
            input.current?.focus();
          }}
          onContext={(i, r) => {
            if (current !== results || !rowContext) return;
            resetPointer();
            setActive(i);
            const scope = rowScope, visibility = copyVisibility.current, request = ++contextRequest.current;
            const owns = () => scope === latestRowScope.current && visibility === copyVisibility.current && request === contextRequest.current && latestHighlight.current === choiceKey([{ ...r, locator: interpretation?.locator }]);
            void rowContext(selected.has(idKey(r.id))).then((action) => {
              if (!owns()) return;
              report("rowContext", false);
              if (action === "toggle") toggle(r);
              else if (action === "copy") requestCopy(selected.size > 0 || list ? chosenKey : choiceKey([{ ...r, locator: interpretation?.locator }]));
              if (action !== null || document.hasFocus()) input.current?.focus({ preventScroll: true });
            }).catch(() => { if (owns()) { report("rowContext"); if (document.hasFocus()) input.current?.focus({ preventScroll: true }); } });
          }}
          onToggle={(i, r) => {
            setPendingCopy(null);
            if (current !== results) return;
            pointer.choose(i);
            toggle(r);
            input.current?.focus();
          }} />}
        {!busy && emptyState}
      </div>
      </div>
      {/* Keep the selection live region mounted when choices remove citation controls. */}
      <CitationShelf announce={selectionChanged} selected={showCitations ? selected : noSelection} removeLabel={removeLabel} onRemove={(r) => {
        toggle(r);
        input.current?.focus({ preventScroll: true });
      }} onRemoveLocator={(r) => changeLocator(r, null)} highlightedReference={selected.size === 0 ? chosen[0] : undefined}
        onChangeReference={changeReference} style={style} keys={keyFormat !== null}
        shown={showCitations ? displayedOutput : undefined} problem={previewProblem} focusable={selected.size > 0} fallbackFocusRef={input} />
      <PickerFooter library={library} groupName={groupName}
        hints={{ enter: enterAction, shiftEnter: shiftEnterAction, escape: escapeAction }}
        altHeld={altHeld} actions={{ copy: copyChosen, add: toggleHighlighted, escape: escapeStep, keys: openKeys }}
        more={
          <PickerMenu isOpen={menuOpen} onOpenChange={(open) => (open ? openMore() : closeMenu(false))}
            buttonRef={moreButton} searchRef={input} library={library} libraryDetail={libraryDetail}
            lastOutcome={lastOutcome}
            zotero={library?.zotero && chooseZoteroLibraries ? {
              offered: ["user" as const, ...offeredGroups.map((id) => ({ group: id }))],
              isChosen, chosenCount: chosenLibraries.length,
              name: (choice) => (typeof choice === "object" ? groupName(choice.group) : pickerText.myLibrary),
              toggle: toggleLibrary,
            } : null}
            density={density} largerText={largerText} searchMark={searchMark} showPastePreview={showPastePreview} pinned={pinned}
            startAtLogin={loginItemOffered && startAtLogin ? { item: startAtLogin, toggle: toggleStartAtLogin } : null}
            updates={updateMenu(checkForUpdates && (() => runFromMenu("checkForUpdates", checkForUpdates)))}
            actions={{
              choose: () => runFromMenu("choose", chooseLibrary),
              useZotero: () => runFromMenu("zotero", useZotero),
              reload: () => runFromMenu("reload", reloadLibrary),
              density: changeDensity,
              largerText: toggleLargerText,
              searchMark: toggleSearchMark,
              preview: togglePastePreview,
              pin: togglePin,
              keys: openKeys,
              settings: openSettings ? () => runFromMenu("settings", () => openSettings(null)) : null,
              forgetApps: destination.forgetAll,
              quit: () => runFromMenu("quit", quitAfterSaving),
            }} />
        } />
      {keysOpen && <KeysDialog bindings={bindings} isMac={isMac} onClose={closeKeys} />}
    </main>
  );
}
