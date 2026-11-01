import * as nativeApi from "./api";
import { keyText, libraryMessages, operationFailures as FAILURE_MESSAGES, pickerText } from "./messages";
import { KEY_TABLE, keysFor, type KeyBinding } from "./keys";
import { useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { LibrarySetup, SampleLibraryExit } from "./LibrarySetup";
import { focusChoices } from "./ui/ChoiceList";
import { zoteroLibraryNames } from "./LibraryStatusLine";
import { usePointerSelection } from "./pointerSelection";
import { Button } from "./ui/Button";
import { usePickerAppearance } from "./ui/appearance";
import { KeysDialog } from "./features/picker/KeysDialog";
import { isMac } from "./platform";
import { choiceKey, idKey, styleOf, type Choice, type CitationReference } from "./features/picker/choice";
import { enterLabel } from "./features/picker/enterLabel";
import { useDeliveryTarget } from "./features/picker/useDeliveryTarget";
import { composing, isFormatShortcut, isKeysShortcut, isMenuShortcut, isPinShortcut, isPreviewShortcut, isQuitShortcut, isSettingsShortcut, isSourceShortcut, isChooseSourceShortcut } from "./features/picker/shortcuts";
import { PickerHeader } from "./features/picker/PickerHeader";
import { Notices, libraryOperations, type Operation } from "./features/picker/Notices";
import { CitationShelf } from "./features/picker/CitationShelf";
import { ResultList } from "./features/picker/ResultList";
import { closestMatchWarnings } from "./features/picker/rows";
import { EmptyState } from "./features/picker/EmptyState";
import { PickerLibraryStatus } from "./features/picker/PickerLibraryStatus";
import { useLibraryStatus } from "./features/picker/useLibraryStatus";
import { SourceIcon, type SourceActionProps } from "./ui/SourceAction";
import { useSourceOpen } from "./features/picker/useSourceOpen";
import { PickerScroll } from "./features/picker/PickerScroll";
import { useWindowResetUndo } from "./features/picker/useWindowResetUndo";
import { UndoNotice } from "./ui/UndoNotice";
import { PickerFooter } from "./features/picker/PickerFooter";
import { PickerMenu } from "./features/picker/PickerMenu";
import { useOperations } from "./features/picker/useOperations";
import { usePickerPreferences } from "./features/picker/usePickerPreferences";
import { useRecentReferences } from "./features/picker/useRecentReferences";
import { useLibrarySearch } from "./features/picker/useLibrarySearch";
import { useCitationOutput } from "./features/picker/useCitationOutput";
import { useZoteroScope } from "./features/picker/useZoteroScope";
import { useDestination } from "./features/picker/useDestination";
import { monitorUpdateOfferInput } from "./features/picker/updateOfferInput";
import { useShortcutStatus } from "./features/picker/useShortcutStatus";
import { usePin } from "./features/picker/usePin";
import { initialSession } from "./features/picker/session";
import { createSessionController } from "./features/picker/sessionController";
import {
  type Copied,
  type Locator,
  type SearchResult,
} from "./api";

const noLibrary = () => null;
const noSelection = new Map<string, CitationReference>();

// A stable provider lets component tests use synthetic data without IPC.
// Formatted citations are offered only when the provider can supply them.
export type PickerApi = Pick<typeof nativeApi,
  "chooseLibrary" | "useZotero" | "libraryStatus" | "reloadLibrary" | "previewFileCitation" | "copyFileCitation" | "copyFormat" | "pickerPreferences" | "setPickerPreferences" |
  "hidePicker" | "pickerPresentation" | "pickerVisible" | "setPinned" | "onLibraryChanged" | "onPickerShown" | "onPickerHidden" | "quit" | "search" | "shortcutStatus"> &
  Partial<Pick<typeof nativeApi, "onPickerPreparing" | "recentReferences" | "openLibrarySource" | "librarySource" | "useSampleLibrary" | "useZoteroLibraries" | "zoteroGroups" | "pickerDestination" | "rememberAppOutput" | "onWindowSizeSaved" | "readWindowPositionUndo" | "undoWindowPosition" | "dismissWindowPositionUndo" | "onWindowReset" | "findSources" | "openFoundLibrary" | "rowContext" | "showCopiedNotice" | "loginItem" | "setLoginItem" | "onUpdateProgress" | "updateChecks" | "setUpdateChecks" | "onUpdateChecks" | "updateAvailable" | "onUpdateAvailable" | "updatePickerActivity" | "checkForUpdates" | "onShortcutStatus" | "openSettings" | "onPreferencesChanged">>;

export type PickerPreview = { query?: string; copied?: Copied };

// The picker: search, choose one or more references, copy them as a citation.
export function App({ api: sourceApi = nativeApi, preview }: { api?: PickerApi; preview?: PickerPreview } = {}) {
  // Visibility is unknown until the native read/event below. Stay idle if
  // that read never returns, including when Ibis starts as a login item.
  useLayoutEffect(() => {
    document.documentElement.dataset.pickerHidden = "";
    return () => { delete document.documentElement.dataset.pickerHidden; };
  }, []);
  const api = usePickerAppearance(sourceApi);
  const { chooseLibrary, useZotero, reloadLibrary,
    hidePicker, pickerVisible, setPinned, onPickerPreparing, onPickerShown, onPickerHidden, search, shortcutStatus, previewFileCitation, copyFileCitation,
    useZoteroLibraries: chooseZoteroLibraries, zoteroGroups } = api;
  const { rowContext, findSources, openFoundLibrary, showCopiedNotice, onWindowSizeSaved, onShortcutStatus, openSettings, onPreferencesChanged } = api;
  const [updateProgress, setUpdateProgress] = useState<nativeApi.UpdateProgress | null>(null);
  const setupApi = useMemo(() => ({ chooseLibrary, useZotero, useSampleLibrary: api.useSampleLibrary, findSources, openFoundLibrary }),
    [chooseLibrary, useZotero, api.useSampleLibrary, findSources, openFoundLibrary]);
  const [session] = useState(() => createSessionController(initialSession(preview?.query, preview?.copied)));
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot);
  const { query, interpretLocators, active, selected, selectionChanged, pendingCopy, copied, copyError, lastOutcome, restored } = state;
  const send = session.dispatch;
  const setQuery = (value: string) => send({ type: "query", value });
  const setActive = (index: number) => send({ type: "highlight", index });
  const cancelCopy = () => send({ type: "cancel" });
  const isCurrentSession = (id: number) => session.getSnapshot().session === id;
  const pointer = usePointerSelection(active, setActive);
  const { resetPointer } = pointer;
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
  const preferences = usePickerPreferences(api, { styles: true, report, perform, presentation: state.visibility });
  const { density, searchIcon, showPastePreview, togglePastePreview, changeDensity, quitAfterSaving: quitPreferences } = preferences;
  const deliveryTarget = useDeliveryTarget(api);
  const destination = useDestination(deliveryTarget.api, { fallback: preferences.format, fallbackReady: preferences.available, chooseDefault: preferences.chooseFormat, report });
  const { format } = destination;
  const quitAfterSaving = () => destination.quitAfterSaving(quitPreferences);
  const style = styleOf(format);
  const { library, libraryVersion, refreshSnapshot } = useLibraryStatus(api, report, cancelCopy);
  const [choosingLibrary, setChoosingLibrary] = useState(false);
  const chooserRequest = useRef<object | null>(null);
  const [welcomeShown, setWelcomeShown] = useState(false);
  const busy = library?.state === "loading" || library?.state === "reloading";
  const blocked = library?.state === "error" && !library.has_snapshot;
  const libraryChoices = choosingLibrary || library?.state === "none" || blocked || library?.state === "loaded" && library.has_snapshot && library.references === 0;
  const showCitations = !libraryChoices;
  const recents = useRecentReferences(api.recentReferences, destination.appId, density === "compact", state.recentCopy, state.session);
  const { queryKey, results, current, interpretation, list, announcement, resultsQuery, error, pending: searchPending } =
    useLibrarySearch({ search, emptySearch: recents.load, query, interpretLocators, library, statusFailed: failures.status ?? false, libraryVersion, sessionId: state.session, isCurrentSession, setActive, refreshSnapshot });
  // biome-ignore lint/correctness/useExhaustiveDependencies: new results are the trigger: the pointer must move again before it counts.
  useEffect(resetPointer, [results, resetPointer]);
  // The selection, otherwise the current pasted list or highlighted result.
  const chosen = !showCitations ? [] : selected.size > 0 ? [...selected.values()] : list ? current : current.slice(active, active + 1).map((r) => ({ ...r, locator: interpretation?.locator }));
  const { formatted, previewReceipt, chosenKey, keyFormat, shown, previewProblem, displayedOutput } =
    useCitationOutput({ chosen, library, libraryVersion, sessionId: state.session, isCurrentSession, format, style, showPastePreview, previewFileCitation, refreshSnapshot });
  const citationProblem = displayedOutput === "failed" ? previewProblem ?? pickerText.previewError : null;
  const input = useRef<HTMLInputElement>(null);
  const formatButtonRef = useRef<HTMLButtonElement>(null);
  const moreButton = useRef<HTMLButtonElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [libraryMenuOpen, setLibraryMenuOpen] = useState(false);
  const [formatOpen, setFormatOpen] = useState(false);
  const { pinned, currentPin, togglePin } = usePin({ setPinned, perform });
  const [keysOpen, setKeysOpen] = useState(false);
  const restoreChooserFocus = useRef(false);
  useLayoutEffect(() => {
    if (choosingLibrary || !restoreChooserFocus.current) return;
    restoreChooserFocus.current = false;
    // Welcome hides Search with CSS. Inspect the committed screen, after the
    // chooser leaves, so focus never goes to the header it just replaced.
    const welcome = input.current?.closest("main")?.querySelector(".library-welcome");
    (welcome ? moreButton.current : input.current)?.focus();
  }, [choosingLibrary]);
  const windowUndo = useWindowResetUndo(api);
  useEffect(() => { if (windowUndo.ticket !== null) { setKeysOpen(false); setMenuOpen(false); setLibraryMenuOpen(false); } }, [windowUndo.ticket]);
  const updateInputBlocked = useRef(false);
  updateInputBlocked.current = Boolean(query || pendingCopy || menuOpen || libraryMenuOpen || choosingLibrary || formatOpen || keysOpen || selected.size || state.activeCopy !== null);
  const updateInput = useRef<ReturnType<typeof monitorUpdateOfferInput> | null>(null);
  useEffect(() => {
    if (!api.updatePickerActivity) return;
    const monitor = monitorUpdateOfferInput(window, api.updatePickerActivity, () =>
      updateInputBlocked.current || !document.hasFocus() || document.documentElement.dataset.pickerHidden !== undefined ||
      [...document.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>('input:not([type]), input[type="text"], input[type="search"], textarea')].some((input) => input.value.length > 0) ||
      [...document.querySelectorAll('[contenteditable="true"]')].some((input) => Boolean(input.textContent?.length)));
    updateInput.current = monitor;
    const visibility = new MutationObserver(monitor.activity);
    visibility.observe(document.documentElement, { attributes: true, attributeFilter: ["data-picker-hidden"] });
    return () => { visibility.disconnect(); monitor.stop(); updateInput.current = null; };
  }, [api.updatePickerActivity]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: each interaction transition invalidates the old idle ticket, including close/reopen; the monitor reads current blocking state through a ref.
  useEffect(() => { updateInput.current?.activity(); }, [query, pendingCopy, menuOpen, libraryMenuOpen, choosingLibrary, formatOpen, keysOpen, selected.size, state.activeCopy, state.visibility]);
  // Where focus goes back to when the keys overlay closes.
  const keysReturn = useRef<HTMLElement | null>(null);
  // While Alt (⌥) is held, the footer shows the other keys instead of Enter.
  const [altHeld, setAltHeld] = useState(false);
  // Enter copies only from the search box; elsewhere it presses the focused control.
  const [searchFocused, setSearchFocused] = useState(false);
  // autoFocus can happen before React listens for focus events.
  useEffect(() => setSearchFocused(document.activeElement === input.current), []);
  const snapshotRevision = library?.revision;
  const highlightedChoice = choiceKey(results.slice(active, active + 1).map((r) => ({ ...r, locator: interpretation?.locator })));
  const latestHighlight = useRef(highlightedChoice);
  latestHighlight.current = highlightedChoice;
  const defaultChoice = list ? choiceKey(current) : highlightedChoice;
  // The controller receives readiness from the existing search/preview I/O
  // adapters. Session and interaction tickets reject an obsolete render.
  useLayoutEffect(() => {
    session.connect({ copyFileCitation, hidePicker, pickerPresentation: api.pickerPresentation, showCopiedNotice, output: destination.copyToken,
      pinned: () => currentPin.current, consume: destination.consumeOnce, refresh: refreshSnapshot,
      askFormat: () => formatButtonRef.current?.click(), reportHide: (failed) => report("hide", failed) });
    send({ type: "view", session: state.session, interaction: state.interaction,
      view: { appId: destination.appId, queryKey, format, snapshotRevision, defaultChoice, chosen, shown, previewVisible: showPastePreview, searchPending,
        ready: destination.ready && preferences.ready, saving: destination.saving, shouldAsk: destination.shouldAsk,
        output: destination.copyToken() } });
  });
  // Setup failures belong to the setup screen; they go when a library is chosen.
  const setupDone = library !== undefined && library?.state !== "none";
  useEffect(() => {
    if (setupDone) setFailures(({ findSources: _found, openFound: _opened, ...rest }) => rest);
  }, [setupDone, setFailures]);
  const sample = Boolean(library?.sample);
  const previousSample = useRef(sample);
  useEffect(() => {
    if (previousSample.current === sample) return;
    previousSample.current = sample;
    send({ type: "sample" });
    input.current?.focus();
  }, [sample, send]);
  const requestCopy = (choice: string | null) => send({ type: "enter", choice, output: destination.copyToken() });
  const hide = () => {
    const presentation = session.getSnapshot().presentation;
    send({ type: "hiding" });
    void perform("hide", () => hidePicker(presentation));
  };
  const chooseFormat = (choice: Choice) => {
    destination.choose(choice);
    send({ type: "question", format: choice, output: destination.copyToken(), answer: destination.askPaste ? "cancel" : "answered" });
  };
  const questionScope = (action: () => void, keepOpen = false) => {
    action();
    send({ type: "question", format: null, output: destination.copyToken(), answer: keepOpen ? "open" : destination.askPaste ? "cancel" : "answered" });
    if (keepOpen && session.getSnapshot().pendingCopy?.question === "open") formatButtonRef.current?.click();
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
    let mounted = true;
    let visibilityEvents = 0;
    let focusAfterActivation = false;
    let searchAtShow = "";
    const subscriptionFailure = (operation: Operation) => {
      if (mounted) report(operation);
      else console.error(FAILURE_MESSAGES[operation]);
      return () => undefined;
    };
    const selectSearch = () => {
      input.current?.focus();
      input.current?.select();
    };
    const activated = (event: FocusEvent) => {
      if (event.target !== event.currentTarget || !focusAfterActivation) return;
      focusAfterActivation = false;
      const search = input.current;
      if (!search) return;
      search.focus();
      if (search.value === searchAtShow) search.select();
      else search.setSelectionRange(search.value.length, search.value.length);
    };
    window.addEventListener("focus", activated);
    let prepared = false;
    const focusSearch = (presentation: number) => {
      searchAtShow = input.current?.value ?? "";
      resetPointer();
      const previousQuery = session.getSnapshot().query;
      send({ type: "shown", now: performance.now(), presentation });
      if (session.getSnapshot().query !== previousQuery) searchAtShow = session.getSnapshot().query;
      if (!prepared) destination.read(true, session.getSnapshot().permissionDetour);
      prepared = false;
      readShortcut();
      // On X11, showing can precede native activation. WebKit restores the
      // previous control then, so finish Search focus once activation arrives.
      focusAfterActivation = !document.hasFocus();
      selectSearch();
    };
    // Every hide is reported, including the native one when focus moves away;
    // a monotonic clock keeps clock changes from clearing or keeping a search.
    const unlistenHidden = onPickerHidden(() => {
      if (!mounted) return;
      visibilityEvents += 1;
      focusAfterActivation = false;
      // A hidden native window may not throttle the web view: pause looping animations.
      document.documentElement.dataset.pickerHidden = "";
      resetPointer();
      send({ type: "hidden", now: performance.now() });
      setKeysOpen(false);
      setLibraryMenuOpen(false);
      setMenuOpen(false);
      setAltHeld(false);
    })
      .catch(() => subscriptionFailure("pickerEvents"));
    // The window appears after this: settle what its first frame shows.
    const unlistenPreparing = onPickerPreparing?.(() => {
      if (!mounted) return;
      prepared = true;
      send({ type: "preparing", now: performance.now() });
      destination.read(true, session.getSnapshot().permissionDetour);
    }).catch(() => subscriptionFailure("pickerEvents"));
    const unlisten = onPickerShown((presentation) => {
      if (!mounted) return;
      visibilityEvents += 1;
      delete document.documentElement.dataset.pickerHidden;
      focusSearch(presentation);
    }).catch(() => subscriptionFailure("pickerEvents"));
    void Promise.all([unlistenHidden, unlisten]).then(async () => {
      if (!mounted) return;
      send({ type: "initialize" });
      const events = visibilityEvents;
      const visible = await pickerVisible();
      // A newer event wins over this read; failed reads leave the flag alone.
      if (!mounted || events !== visibilityEvents) return;
      if (visible) delete document.documentElement.dataset.pickerHidden;
      else document.documentElement.dataset.pickerHidden = "";
    }).catch(() => undefined);
    destination.read();
    preferences.loadPreferences(() => mounted);
    // Choices made in Settings apply to the picker at once.
    const unlistenPreferences = onPreferencesChanged?.(() => { preferences.loadPreferences(() => mounted); destination.read(false); })
      .catch(() => undefined);
    return () => {
      mounted = false;
      window.removeEventListener("focus", activated);
      send({ type: "retire" });
      void unlistenHidden.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
      void unlisten.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
      void unlistenPreparing?.then((stop) => stop?.());
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
    if (!moved && (citationProblem || selected.size === 0 || document.activeElement !== input.current)) return;
    document.getElementById(`result-${active}`)?.scrollIntoView?.({ block: "nearest" });
  }, [active, pointer.cameFrom, selected.size, citationProblem,
    selected.size > 0 ? formatted : null, selected.size > 0 ? format : null, selected.size > 0 ? searchFocused : null]);

  // Content changes preserve Search's keyboard highlight. A real viewport
  // resize must also leave the selected title readable while Page is edited.
  useEffect(() => {
    const area = resultArea.current;
    if (!area) return;
    const scrollers = [area, area.parentElement].filter((node): node is HTMLElement => node !== null);
    const reveal = (viewport = false) => {
      // A refusal owns the middle viewport. Read the committed DOM so even a
      // queued observer cannot undo its reveal before effects are refreshed,
      // or scroll past the person's newer reading position. Explicit arrows
      // still reveal the highlighted row through the effect above.
      if (area.parentElement?.querySelector(":scope > .notices > .citation-problem")) return;
      if (!viewport && (!followKeyboard.current || document.activeElement !== input.current)) return;
      const row = document.getElementById(`result-${active}`);
      if (!row) return;
      for (const scroller of scrollers) {
        if (scroller.clientHeight <= 0 || !/auto|scroll/.test(getComputedStyle(scroller).overflowY)) continue;
        const top = scroller.getBoundingClientRect().top + scroller.clientTop;
        const bottom = top + scroller.clientHeight;
        const whole = row.getBoundingClientRect();
        const item = whole.height > scroller.clientHeight
          ? row.querySelector(".result-title")?.getBoundingClientRect() ?? whole : whole;
        // A tall row already filling the scrollport cannot be shown any fuller.
        if (item.top <= top && item.bottom >= bottom) continue;
        if (item.bottom > bottom) scroller.scrollTop += Math.ceil(item.bottom - bottom);
        else if (item.top < top) scroller.scrollTop += Math.floor(item.top - top);
      }
    };
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => reveal());
    for (const scroller of scrollers) observer?.observe(scroller);
    const resized = () => reveal(true);
    window.addEventListener("resize", resized);
    return () => { observer?.disconnect(); window.removeEventListener("resize", resized); };
  }, [active]);

  // Success is brief; warnings stay available until explicitly dismissed.
  useEffect(() => {
    if (preview || !copied || copied.warnings.length > 0) return;
    const timer = window.setTimeout(() => send({ type: "dismiss", copied }), 2400);
    return () => window.clearTimeout(timer);
  }, [copied, preview, send]);

  const toggle = (result?: SearchResult) => send({ type: "toggle", result, locator: interpretation?.locator });
  const changeLocator = (reference: CitationReference, locator: Readonly<Locator> | null) => {
    send({ type: "reference", reference, change: { locator: locator ? { ...locator } : null }, selectedOnly: true });
    input.current?.focus({ preventScroll: true });
  };
  const changeReference = (reference: CitationReference, change: Pick<CitationReference, "locator" | "form">) => {
    send({ type: "reference", reference, change, initialSelection: list ? current : undefined });
  };

  const rowScope = useMemo(() => ({ api, current, selected, queryKey, format, snapshotRevision }), [api, current, selected, queryKey, format, snapshotRevision]);
  const latestRowScope = useRef(rowScope);
  latestRowScope.current = rowScope;
  const contextRequest = useRef(0);

  const copyChosen = () => {
    if (chosen.length > 0) requestCopy(chosenKey);
    else if (showCitations && searchPending) requestCopy(null);
  };
  // Nothing matched this search, so the empty state offers what to try next.
  const nothingMatched = Boolean(query) && resultsQuery === queryKey && results.length === 0 && !error;
  const { chosenLibraries, severalLibraries, isChosen, toggleLibrary, searchAllLibraries, groupName, offeredGroups, libraryOf, narrowed } =
    useZoteroScope({ library, libraryVersion, menuOpen: libraryMenuOpen, nothingMatched, zoteroGroups, chooseZoteroLibraries, report, perform,
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
    cancelCopy();
    setLibraryMenuOpen(false);
    // React Aria records the invoker before mounting its modal focus scope.
    moreButton.current?.focus();
    setMenuOpen(true);
  };
  const closeMenu = (refocus: boolean) => {
    setMenuOpen(false);
    setLibraryMenuOpen(false);
    if (refocus) moreButton.current?.focus();
  };
  const focusPicker = () => (welcomeShown ? moreButton.current : input.current)?.focus();
  const runFromMenu = (operation: Operation, action: () => Promise<unknown>) => {
    cancelCopy();
    closeMenu(false);
    focusPicker();
    void perform(operation, action);
  };
  const sourceItem = !libraryChoices ? current[active]?.id ?? (selected.size === 1 ? [...selected.values()][0]?.id : null) ?? null : null;
  const sourceOpen = useSourceOpen(api, library, state.visibility, sourceItem, sourceItem === null, refreshSnapshot);
  const chooseSources = () => {
    cancelCopy(); closeMenu(false); setKeysOpen(false);
    restoreChooserFocus.current = false;
    chooserRequest.current = {};
    setChoosingLibrary(true);
  };
  const dismissChooser = () => {
    chooserRequest.current = null;
    restoreChooserFocus.current = true;
    setChoosingLibrary(false);
  };
  const performSourceChoice: typeof perform = (operation, action, isCurrent) => {
    if (operation === "findSources") return perform(operation, action, isCurrent);
    const request = {};
    const visibility = session.getSnapshot().visibility;
    chooserRequest.current = request;
    const owns = () => chooserRequest.current === request && Boolean(input.current?.isConnected) &&
      visibility === session.getSnapshot().visibility && session.getSnapshot().hiddenAt === null && (isCurrent?.() ?? true);
    let accepted = false;
    return perform(operation, async () => {
      await action();
      accepted = true;
    }, owns).then(() => {
      // Only this chooser's latest accepted selection can dismiss it. Watcher
      // statuses and completions from a dismissed presentation have no claim.
      if (accepted && owns()) dismissChooser();
    });
  };
  const chooseFile = () => runFromMenu("choose", chooseLibrary);
  const missingSelected = sourceItem !== null && library?.stale && library.problem === "missing" && !library.zotero;
  const openSource = (appOnly = false, reveal = altHeld) => {
    cancelCopy();
    closeMenu(false);
    setKeysOpen(false);
    focusPicker();
    if (missingSelected && !appOnly) { chooseFile(); return; }
    if (sourceOpen.failure && !sourceOpen.failure.changed && !appOnly) {
      if (sourceOpen.failure.kind === "file") chooseFile();
      else chooseSources();
    } else sourceOpen.run(reveal, appOnly);
  };
  const source = sourceOpen.source;
  const sourceLabel = missingSelected ? pickerText.findSourceFile(library?.name ?? null) : sourceOpen.failure && !sourceOpen.failure.changed ? sourceOpen.failure.kind === "file" ? pickerText.findSourceFile(source.name) : pickerText.chooseLibraryMenu
    : source.kind === "zotero" ? sourceItem ? pickerText.showInZotero : pickerText.openZotero
    : altHeld ? pickerText.showSourceFolder(isMac) : pickerText.openSource(source.name);
  const sourceAction: SourceActionProps = { label: sourceLabel, keys: keysFor(altHeld && source.kind === "file" ? "revealSource" : "source"), icon: source.icon,
    file: source.kind === "file", busy: sourceOpen.busy, open: () => openSource() };
  const sourceChoice = { ...sourceAction, icon: <SourceIcon icon={source.icon} file={source.kind === "file"} /> };
  const openKeys = () => {
    cancelCopy();
    closeMenu(false);
    const focused = document.activeElement;
    // A dismissed menu item returns to a visible control; welcome has no Search.
    keysReturn.current = focused instanceof HTMLElement && !focused.closest("[role='menu']") ? focused
      : welcomeShown ? moreButton.current : input.current;
    setKeysOpen(true);
  };
  const closeKeys = () => {
    setKeysOpen(false);
    const back = keysReturn.current;
    const fallback = welcomeShown ? moreButton.current : input.current;
    (back?.isConnected ? back : fallback)?.focus();
  };
  // The keys overlay lists the global shortcut too, when it works.
  const bindings: readonly KeyBinding[] = shortcut?.registered
    ? [...KEY_TABLE, { region: "system", action: "open", keys: [shortcut.shortcut], displayLabel: shortcut.desktop ? shortcut.shortcut : undefined }]
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
  const escapeAction = keysOpen ? keyText.close : menuOpen ? keyText.closeMenu : query && !welcomeShown ? keyText.clearSearch : keyText.hide;
  const escapeStep = () => {
    cancelCopy();
    // Escape steps back one layer, retaining the chosen references.
    if (keysOpen) closeKeys();
    else if (menuOpen) closeMenu(true);
    else if (choosingLibrary) dismissChooser();
    else if (query && !welcomeShown) {
      send({ type: "escape-search" });
      input.current?.focus();
    } else hide();
  };
  const toggleHighlighted = () => {
    toggle(highlighted);
    input.current?.focus();
  };
  const emptyState = choosingLibrary ? <LibrarySetup api={setupApi} perform={performSourceChoice} autoFocus
    shortcut={shortcut} onWelcomeChange={setWelcomeShown} /> : (library !== undefined && (libraryChoices || busy || results.length === 0 && !error && (!library?.error || blocked)) ? (
    <EmptyState library={library} query={query} narrowed={narrowed}
      source={api.openLibrarySource ? sourceChoice : undefined}
      onChooseLibrary={() => void perform("choose", chooseLibrary)}
      onClearSearch={() => { setQuery(""); input.current?.focus(); }}
      onSearchAll={searchAllLibraries}
      onReload={() => { input.current?.focus(); void perform("reload", reloadLibrary); }}
      setup={
        (focus, empty) => <LibrarySetup api={setupApi} perform={perform} autoFocus={focus} empty={empty} shortcut={shortcut} onWelcomeChange={setWelcomeShown} />
      } />
  ) : null);

  useEffect(() => {
    let current = true;
    const subscription = onWindowSizeSaved?.((saved) => { if (current) report("windowSize", !saved); });
    void subscription?.catch(() => { if (current) report("windowSize"); });
    return () => { current = false; void subscription?.then((stop) => stop()).catch(() => {}); };
  }, [onWindowSizeSaved, report]);

  return (
    <main
      data-selection={showCitations && selected.size > 0 || undefined}
      onFocus={(e) => setSearchFocused(e.target === input.current)}
      onBlur={() => setSearchFocused(false)}
      onKeyDownCapture={(e) => {
        pointer.resetHover();
        if (composing(e)) return;
        // Format owns its modal exit and restores Search before running a
        // command. Let its capture handler consume these keys first.
        if (e.target instanceof Element && (e.target.closest(".toolbar-menu-popover") ||
          formatButtonRef.current?.contains(e.target) && formatButtonRef.current.getAttribute("aria-expanded") === "true")) return;
        if (isChooseSourceShortcut(e)) {
          e.preventDefault(); e.stopPropagation();
          if (!e.repeat) chooseSources();
        } else if (isSourceShortcut(e)) {
          e.preventDefault();
          e.stopPropagation();
          if (!e.repeat) openSource(false, e.altKey);
        }
      }}
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
      <PickerHeader
        pinned={pinned} icon={searchIcon} onTogglePin={() => { togglePin(); input.current?.focus(); }}
        inputRef={input}
        query={query}
        pageToken={usingLocator && <span className="search-page-token" role="group" aria-label={pickerText.usingLocator(usingLocator.label, usingLocator.value)}>
          <Button variant="quiet" size="small" aria-label={pickerText.searchAsTyped} title={pickerText.searchAsTyped}
            onPress={() => { send({ type: "literal" }); input.current?.focus({ preventScroll: true }); }}>
            <bdi>{usingLocator.label === "page" ? "p." : usingLocator.label} {usingLocator.value}</bdi><span aria-hidden="true">×</span>
          </Button>
          {canApplyLocator && <Button variant="quiet" size="small" onPress={() => { changeLocator(prepared, usingLocator); input.current?.focus(); }} aria-label={pickerText.applyLocator(prepared.key)} title={pickerText.applyLocator(prepared.key)}>Apply</Button>}
        </span>}
        onQueryChange={(value) => {
          pointer.resetHover();
          setQuery(value);
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
            cancelCopy();
            followKeyboard.current = true; pointer.forgetClick();
            const step = e.key === "ArrowDown" ? 1 : -1;
            setActive(Math.min(Math.max(session.getSnapshot().active + step, 0), Math.max(results.length - 1, 0)));
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
        formatScope={destination.scope}
        onFormatOpenChange={(open, reason) => {
          setFormatOpen(open);
          if (open || pendingCopy?.question !== "open" || reason === "action") return;
          // Dismissal uses the fallback for this attempt; application commands cancel it.
          send({ type: "question", answer: destination.askPaste || reason === "command" ? "cancel" : "answered" });
        }}
        onceOnly={destination.onceOnly}
        onToggleOnce={() => questionScope(destination.toggleOnce, true)}
        onAutomatic={() => questionScope(destination.restoreAutomatic)}
        onDefault={() => questionScope(destination.useDefault)}
        pasteChoices={destination.pasteChoices}
        onPasteChange={(paste) => {
          send({ type: "question", answer: "cancel" });
          destination.choosePaste(paste);
        }}
        formatDisabled={!destination.ready || destination.saving}
        showFormat={!libraryChoices && (library === null || Boolean(library?.has_snapshot))}
        styles
        onFormatChange={chooseFormat}
        onFormatCommand={(command) => {
          const visibility = session.getSnapshot().visibility;
          return {
            isCurrent: () => visibility === session.getSnapshot().visibility && session.getSnapshot().hiddenAt === null,
            run: () => {
              if (command === "more") openMore();
              else if (command === "keys") openKeys();
              else if (command === "pin") togglePin();
              else if (command === "preview") togglePastePreview();
              else if (command === "quit") void perform("quit", quitAfterSaving);
              else if (command === "chooseSource") chooseSources();
              else if (command === "source" || command === "revealSource") openSource(false, command === "revealSource");
            },
          };
        }}
      />
      <PickerScroll citationProblem={citationProblem} citationProblemIdentity={citationProblem ? formatted?.request : undefined}>
      {shortcut?.error && <div className="notices">
        <p role="status">
          {pickerText.shortcutError(shortcut.registered, shortcut.shortcut, shortcut.error)}
          {shortcut.desktop && !welcomeShown && ` ${shortcut.desktop.guidance}`}
        </p>
      </div>}
      <Notices densityAnnouncement={preferences.densityAnnouncement} updateProgress={updateProgress} library={library} failures={failures}
        error={error} copyError={copyError} copied={copied} missing={list?.missing}
        citationProblem={citationProblem}
        confirmation={state.copyNotice}
        onDismissCopied={() => send({ type: "dismiss" })} />
      <div className="results-area scroll-area" ref={resultArea}>
        {library?.has_snapshot && results.length > 0 && !results.some((r) => r.recent) && !busy && showCitations && <div className="results-heading" aria-hidden="true">
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
            if (action === "toggle") cancelCopy();
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
            const scope = rowScope, visibility = session.getSnapshot().visibility, request = ++contextRequest.current;
            const owns = () => scope === latestRowScope.current && visibility === session.getSnapshot().visibility && request === contextRequest.current && latestHighlight.current === choiceKey([{ ...r, locator: interpretation?.locator }]);
            void rowContext(selected.has(idKey(r.id))).then((action) => {
              if (!owns()) return;
              report("rowContext", false);
              if (action === "toggle") toggle(r);
              else if (action === "copy") requestCopy(selected.size > 0 || list ? chosenKey : choiceKey([{ ...r, locator: interpretation?.locator }]));
              if (action !== null || document.hasFocus()) input.current?.focus({ preventScroll: true });
            }).catch(() => { if (owns()) { report("rowContext"); if (document.hasFocus()) input.current?.focus({ preventScroll: true }); } });
          }}
          onToggle={(i, r) => {
            cancelCopy();
            if (current !== results) return;
            pointer.choose(i);
            toggle(r);
            input.current?.focus();
          }} />}
        {!busy && emptyState}
        {library?.sample && <SampleLibraryExit useSampleLibrary={api.useSampleLibrary} perform={perform} />}
      </div>
      </PickerScroll>
      {/* Keep the selection live region mounted when choices remove citation controls. */}
      <CitationShelf presentation={state.visibility} present={showCitations && results.length > 0} announce={selectionChanged} selected={showCitations ? selected : noSelection} removeLabel={removeLabel} onRemove={(r) => {
        send({ type: "remove-reference", reference: r });
        input.current?.focus({ preventScroll: true });
      }} highlightedReferences={selected.size === 0 ? chosen : undefined} previewReceipt={previewReceipt}
        onChangeReference={changeReference} referenceLibrary={(r) => libraryOf(r.source ?? "")} style={style} keys={keyFormat !== null}
        shown={showCitations ? displayedOutput : undefined} searching={searchPending && showPastePreview} outputWarnings={shown && typeof shown === "object" ? shown.warnings : []}
        problem={previewProblem} copied={showCitations ? state.shelfCopied : null} focusable={selected.size > 0} fallbackFocusRef={input}
        source={source.kind === "zotero" ? { ...sourceAction, label: pickerText.showInZotero } : sourceAction} onOpenSource={(reference) => sourceOpen.run(false, false, reference.id)} />
      <PickerFooter library={library} groupName={groupName}
        status={<PickerLibraryStatus library={library} groupName={groupName} isOpen={libraryMenuOpen}
          onOpenChange={(open) => { if (open) { cancelCopy(); setMenuOpen(false); } setLibraryMenuOpen(open); }}
          searchRef={input} sourceAction={sourceAction} sourceFailure={sourceOpen.failure !== null}
          statusIcon={source.status_icon ?? null}
          zotero={library?.zotero && chooseZoteroLibraries ? {
            offered: ["user" as const, ...offeredGroups.map((group) => ({ group }))], isChosen, chosenCount: chosenLibraries.length,
            name: (choice) => typeof choice === "object" ? groupName(choice.group) : pickerText.myLibrary, toggle: toggleLibrary, all: searchAllLibraries,
          } : null}
          actions={{ choose: chooseSources, find: chooseFile, reload: () => runFromMenu("reload", reloadLibrary),
            useZotero: () => runFromMenu("zotero", useZotero), openZotero: () => openSource(source.kind !== "zotero") }}
          groupsFailed={failures.zoteroGroups} failure={sourceOpen.failure?.message ?? [...libraryOperations].filter((operation) => operation !== "zoteroGroups" && failures[operation]).map((operation) => FAILURE_MESSAGES[operation]).join(" ")} />}

        hints={{ enter: enterAction, shiftEnter: shiftEnterAction, escape: escapeAction }}
        altHeld={altHeld} actions={{ copy: copyChosen, add: toggleHighlighted, escape: escapeStep }}
        more={
          <PickerMenu isOpen={menuOpen} onOpenChange={(open) => (open ? openMore() : closeMenu(false))}
            buttonRef={moreButton} searchRef={input} libraryDetail={libraryDetail}
            source={api.openLibrarySource ? sourceAction : undefined}
            lastOutcome={lastOutcome}
            density={density} showPastePreview={showPastePreview} pinned={pinned}
            actions={{
              density: changeDensity,
              preview: togglePastePreview,
              pin: togglePin,
              keys: openKeys,
              settings: openSettings ? () => runFromMenu("settings", () => openSettings(null)) : null,
              copyOnlyHere: destination.pasteChoices && destination.app ? { app: destination.app, choose: () => {
                cancelCopy();
                destination.choosePaste("copy-only");
              } } : null,
              forgetRecent: recents.forget ? () => { report("recentHistory", !recents.forget!()); input.current?.focus(); } : null,
              quit: () => runFromMenu("quit", quitAfterSaving),
            }} />
        } />
      {keysOpen && <KeysDialog bindings={bindings} isMac={isMac} onClose={closeKeys} />}
      {(state.removed || windowUndo.ticket !== null) && <div className="undo-notices" role="region" aria-label={pickerText.undo}>
        {state.removed && <UndoNotice key={`reference-${state.removed.token}`} message="Removed from citation." paused={state.hiddenAt !== null} fallbackFocusRef={input}
          onExpire={() => send({ type: "expire-removal", removal: state.removed! })}
          onUndo={async () => { send({ type: "undo-reference", removal: state.removed! }); }} />}
        {windowUndo.ticket !== null && <UndoNotice key={`window-${windowUndo.ticket}`} message={pickerText.windowReset} onUndo={windowUndo.undo} onExpire={windowUndo.expire} paused={state.hiddenAt !== null} fallbackFocusRef={input} />}
      </div>}
    </main>
  );
}
