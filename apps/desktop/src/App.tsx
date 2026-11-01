import * as nativeApi from "./api";
import { useEffect, useLayoutEffect } from "react";
import { pickerText } from "./messages";
import { usePointerSelection } from "./pointerSelection";
import { usePickerAppearance } from "./ui/appearance";
import { choiceKey, styleOf } from "./features/picker/choice";
import { PickerHeader } from "./features/picker/PickerHeader";
import { PickerScroll } from "./features/picker/PickerScroll";
import { Notices } from "./features/picker/Notices";
import { PickerPageToken } from "./features/picker/PickerPageToken";
import { PickerResults } from "./features/picker/PickerResults";
import { PickerCitationShelf } from "./features/picker/PickerCitationShelf";
import { PickerStatusFooter } from "./features/picker/PickerStatusFooter";
import { PickerOverlays } from "./features/picker/PickerOverlays";
import { useOperations } from "./features/picker/useOperations";
import { useShortcutStatus } from "./features/picker/useShortcutStatus";
import { usePickerPreferences } from "./features/picker/usePickerPreferences";
import { useDeliveryTarget } from "./features/picker/useDeliveryTarget";
import { useDestination } from "./features/picker/useDestination";
import { useRecentReferences } from "./features/picker/useRecentReferences";
import { useLibrarySearch } from "./features/picker/useLibrarySearch";
import { useCitationOutput } from "./features/picker/useCitationOutput";
import { useZoteroScope } from "./features/picker/useZoteroScope";
import { usePickerSession } from "./features/picker/usePickerSession";
import { useUpdateProgress } from "./features/picker/useUpdateProgress";
import { usePickerLibrary } from "./features/picker/usePickerLibrary";
import { usePickerMenus } from "./features/picker/usePickerMenus";
import { useUpdateOfferActivity } from "./features/picker/useUpdateOfferActivity";
import { usePickerSearchFocus } from "./features/picker/usePickerSearchFocus";
import { useCitationCopy } from "./features/picker/useCitationCopy";
import { useLibrarySetup } from "./features/picker/useLibrarySetup";
import { useAltHeld } from "./features/picker/useAltHeld";
import { HeldKeys } from "./features/picker/HeldKeys";
import { useNativeRequests } from "./features/picker/useNativeRequests";
import { usePickerLifecycle } from "./features/picker/usePickerLifecycle";
import { useResultScroll } from "./features/picker/useResultScroll";
import { useCitationFeedback } from "./features/picker/useCitationFeedback";
import { useCitationRows } from "./features/picker/useCitationRows";
import { usePickerSource } from "./features/picker/usePickerSource";
import { usePickerActions } from "./features/picker/usePickerActions";
import { usePickerKeys } from "./features/picker/usePickerKeys";
import { useWindowSizeStatus } from "./features/picker/useWindowSizeStatus";
import type { Copied } from "./api";
import { usePickerDelights } from "./features/picker/usePickerDelights";
import { useEmptyFeedback, useZoomFeedback } from "./ui/Delights";
import { useInterfaceMessages } from "./i18n";

// A stable provider lets component tests use synthetic data without IPC.
// Formatted citations are offered only when the provider can supply them.
export type PickerApi = Pick<typeof nativeApi,
  "chooseLibrary" | "useZotero" | "libraryStatus" | "reloadLibrary" | "previewFileCitation" | "copyFileCitation" | "copyFormat" | "pickerPreferences" | "setPickerPreferences" |
  "hidePicker" | "pickerPresentation" | "pickerVisible" | "setPinned" | "onLibraryChanged" | "onPickerShown" | "onPickerHidden" | "quit" | "search" | "shortcutStatus"> &
  Partial<Pick<typeof nativeApi, "onLibraryFileDropped" | "onFileDrag" | "delightView" | "onPickerPreparing" | "recentReferences" | "openLibrarySource" | "librarySource" | "useSampleLibrary" | "useZoteroLibraries" | "zoteroGroups" | "pickerDestination" | "rememberAppOutput" | "onWindowSizeSaved" | "readWindowPositionUndo" | "undoWindowPosition" | "dismissWindowPositionUndo" | "onWindowReset" | "findSources" | "openFoundLibrary" | "rowContext" | "showCopiedNotice" | "loginItem" | "setLoginItem" | "onUpdateProgress" | "updateChecks" | "setUpdateChecks" | "onUpdateChecks" | "updateAvailable" | "onUpdateAvailable" | "updatePickerActivity" | "checkForUpdates" | "onShortcutStatus" | "openSettings" | "onPreferencesChanged" | "takeSearchRequest" | "onSearchRequested" | "alignmentTick" | "resetWindowPosition">>;

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
  const pickerSession = usePickerSession(preview);
  const { session, state, send, setQuery, setActive, cancelCopy, isCurrentSession } = pickerSession;
  const { query, interpretLocators, active, selected, copied, copyError } = state;
  const { text } = useInterfaceMessages();
  const emptyFeedback = useEmptyFeedback(`${state.visibility}:${state.interaction}`);
  const zoomFeedback = useZoomFeedback(state.presentation !== null);
  const pointer = usePointerSelection(active, setActive);
  const { resetPointer } = pointer;
  const operations = useOperations();
  const { failures, setFailures, report, perform } = operations;
  const { shortcut, readShortcut } = useShortcutStatus({
    shortcutStatus: api.shortcutStatus, onShortcutStatus: api.onShortcutStatus, report,
  });
  const updateProgress = useUpdateProgress(api, setFailures);
  const preferences = usePickerPreferences(api, { styles: true, report, perform, presentation: state.visibility });
  const { density, searchIcon, showPastePreview, togglePastePreview, quitAfterSaving: quitPreferences } = preferences;
  const deliveryTarget = useDeliveryTarget(api);
  const destination = useDestination(deliveryTarget.api, {
    fallback: preferences.format, fallbackReady: preferences.available, chooseDefault: preferences.chooseFormat, report,
  });
  const { format } = destination;
  const quitAfterSaving = () => destination.quitAfterSaving(quitPreferences);
  const libraryState = usePickerLibrary(api, report, cancelCopy);
  const { library, libraryVersion, refreshSnapshot, choosingLibrary, welcomeShown, libraryChoices, showCitations } = libraryState;
  const delights = usePickerDelights(api, state.presentation, library, preferences.smallSurprises);
  useEffect(() => { if (choosingLibrary) delights.dismiss(); }, [choosingLibrary, delights.dismiss]);
  const recents = useRecentReferences(api.recentReferences, destination.appId, density === "compact", state.recentCopy, state.session);
  const searchState = useLibrarySearch({
    search: api.search, emptySearch: recents.load, query, interpretLocators, library, statusFailed: failures.status ?? false,
    libraryVersion, sessionId: state.session, isCurrentSession, setActive, refreshSnapshot,
  });
  const { queryKey, results, current, interpretation, list, error, pending: searchPending } = searchState;
  // biome-ignore lint/correctness/useExhaustiveDependencies: new results are the trigger: the pointer must move again before it counts.
  useEffect(resetPointer, [results, resetPointer]);
  // The selection, otherwise the current pasted list or highlighted result.
  const chosen = !showCitations ? [] : selected.size > 0 ? [...selected.values()] : list ? current
    : current.slice(active, active + 1).map((r) => ({ ...r, locator: interpretation?.locator }));
  const output = useCitationOutput({
    chosen, library, libraryVersion, sessionId: state.session, isCurrentSession, format, style: styleOf(format),
    showPastePreview, previewFileCitation: api.previewFileCitation, refreshSnapshot,
  });
  const { formatted, shown, previewProblem, displayedOutput } = output;
  const citationProblem = displayedOutput === "failed" ? previewProblem ?? pickerText.previewError : null;
  const menus = usePickerMenus(api, operations, libraryState, cancelCopy);
  const { input, formatButtonRef, currentPin, pinned, togglePin, setFormatOpen } = menus;
  useUpdateOfferActivity(api, state, menus, choosingLibrary);
  const { searchFocused, setSearchFocused } = usePickerSearchFocus(input);
  const snapshotRevision = library?.revision;
  const highlightedChoice = choiceKey(results.slice(active, active + 1)
    .map((r) => ({ ...r, locator: interpretation?.locator })));
  const defaultChoice = list ? choiceKey(current) : highlightedChoice;
  const { requestCopy, chooseFormat, questionScope, onFormatOpenChange } = useCitationCopy({
    api, session, state, destination, currentPin, formatButtonRef, refreshSnapshot, report, setFormatOpen,
    view: {
      queryKey, format, snapshotRevision, defaultChoice, chosen, shown, previewVisible: showPastePreview,
      searchPending, ready: preferences.ready,
    },
  });
  useLibrarySetup(library, setFailures, send, input);
  const { altHeld, setAltHeld } = useAltHeld();
  useNativeRequests(api, query, setQuery, input, state.selected.size);
  usePickerLifecycle({ api, pickerSession, menus, destination, preferences, resetPointer, readShortcut, report, setAltHeld });
  const { resultArea, followKeyboard } = useResultScroll({
    input, active, restored: state.restored, pointer, selectedSize: selected.size, citationProblem, formatted, format, searchFocused,
  });
  useCitationFeedback(copied, preview, send);
  const rows = useCitationRows({
    api, pickerSession, searchState, chosenKey: output.chosenKey, chosen, showCitations, format, snapshotRevision,
    input, pointer, requestCopy, report,
  });
  const nothingMatched = Boolean(query) && searchState.resultsQuery === queryKey && results.length === 0 && !error;
  const scope = useZoteroScope({
    library, libraryVersion, menuOpen: menus.libraryMenuOpen, nothingMatched,
    zoteroGroups: api.zoteroGroups, chooseZoteroLibraries: api.useZoteroLibraries, report, perform,
    focusSearch: () => input.current?.focus(),
  });
  const source = usePickerSource({ api, pickerSession, libraryState, menus, current, altHeld, perform });
  const actions = usePickerActions({
    api, pickerSession, menus, libraryState, searchState, destination, shown, chosen, canPaste: deliveryTarget.canPaste,
    shortcut, perform, dismissChooser: source.dismissChooser, toggle: rows.toggle,
  });
  const keys = usePickerKeys({
    api, pickerSession, menus, source, results, showCitations, copyChosen: rows.copyChosen, actions, pointer, followKeyboard,
    togglePastePreview, quitAfterSaving, perform,
    onEmptyEnter: () => {
      if (chosen.length > 0 || searchPending || state.pendingCopy) return;
      delights.dismiss();
      emptyFeedback.refuse(text(library?.state === "loading" || library?.state === "reloading" ? "delight-loading" : !library?.has_snapshot ? "delight-no-library" : "delight-empty"));
    },
  });
  useWindowSizeStatus(api.onWindowSizeSaved, report);

  return (
    <main
      data-selection={showCitations && selected.size > 0 || undefined}
      onFocus={(e) => setSearchFocused(e.target === input.current)}
      onBlur={() => setSearchFocused(false)}
      onKeyDownCapture={keys.onKeyDownCapture}
      onScrollCapture={pointer.resetHover}
      onKeyDown={keys.onKeyDown}
    >
      <PickerHeader
        pinned={pinned} icon={searchIcon} onTogglePin={() => { togglePin(); input.current?.focus(); }}
        onResetPosition={api.resetWindowPosition}
        inputRef={input} query={query}
        pageToken={<PickerPageToken
          usingLocator={actions.usingLocator} prepared={actions.prepared} canApplyLocator={actions.canApplyLocator}
          changeLocator={rows.changeLocator} send={send} input={input} />}
        onQueryChange={(value) => { delights.dismiss(); pointer.resetHover(); setQuery(value); }}
        onSearchKeyDown={keys.onSearchKeyDown}
        expanded={results.length > 0 && showCitations}
        activeDescendant={showCitations && results[active] ? `result-${active}` : undefined}
        format={format} formatButtonRef={formatButtonRef}
        app={destination.app} formatScope={destination.scope}
        onFormatOpenChange={onFormatOpenChange}
        onceOnly={destination.onceOnly}
        onToggleOnce={() => questionScope(destination.toggleOnce, true)}
        onAutomatic={() => questionScope(destination.restoreAutomatic)}
        onDefault={() => questionScope(destination.useDefault)}
        pasteChoices={destination.pasteChoices}
        onPasteChange={(paste) => { send({ type: "question", answer: "cancel" }); destination.choosePaste(paste); }}
        formatDisabled={!destination.ready || destination.saving}
        showFormat={!libraryChoices && (library === null || Boolean(library?.has_snapshot))}
        styles onFormatChange={chooseFormat} onFormatCommand={keys.onFormatCommand}
      />
      <PickerScroll onFileDrag={api.onFileDrag} presentation={state.visibility} citationProblem={citationProblem} citationProblemIdentity={citationProblem ? formatted?.request : undefined}>
        {shortcut?.error && <div className="notices">
          <p role="status">
            {pickerText.shortcutError(shortcut.registered, shortcut.shortcut, shortcut.error)}
            {shortcut.desktop && !welcomeShown && ` ${shortcut.desktop.guidance}`}
          </p>
        </div>}
        <Notices feedback={zoomFeedback} densityAnnouncement={preferences.densityAnnouncement} updateProgress={updateProgress} library={library} failures={failures}
          error={error} copyError={copyError} copied={copied} missing={list?.missing}
          citationProblem={citationProblem} confirmation={state.copyNotice}
          onDismissCopied={() => send({ type: "dismiss" })} />
        <PickerResults
          api={api} libraryState={libraryState} searchState={searchState} state={state} source={source} scope={scope}
          shortcut={shortcut} perform={perform} input={input} pointer={pointer} rows={rows}
          density={density} resultArea={resultArea} setQuery={setQuery} peck={delights.peck} />
      </PickerScroll>
      {/* Keep the selection live region mounted when choices remove citation controls. */}
      <PickerCitationShelf
        state={state} send={send} showCitations={showCitations} results={results} output={output} chosen={chosen}
        changeReference={rows.changeReference} libraryOf={scope.libraryOf} format={format} searchPending={searchPending}
        showPastePreview={showPastePreview} input={input} sourceState={source} />
      <PickerStatusFooter
        api={api} library={library} scope={scope} sourceState={source} menus={menus} actions={actions}
        copyChosen={rows.copyChosen} destination={destination} recents={recents} preferences={preferences}
        operations={operations} altHeld={altHeld} feedback={emptyFeedback.feedback ?? delights.feedback} passive={!emptyFeedback.feedback && delights.passive} emptyAction={emptyFeedback.shaking} lastOutcome={state.lastOutcome}
        cancelCopy={cancelCopy} quitAfterSaving={quitAfterSaving} />
      <PickerOverlays state={state} send={send} menus={menus} bindings={actions.bindings} />
      <HeldKeys />
    </main>
  );
}
