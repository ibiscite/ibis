import * as nativeApi from "./api";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { idKey } from "./features/picker/choice";
import { keysFor, matchesAccelerator } from "./keys";
import { isMac } from "./platform";
import { PeekCard } from "./ui/PeekCard";

import { peekText, pickerText, rowText } from "./messages";
import { usePointerSelection } from "./pointerSelection";
import { usePickerAppearance } from "./ui/appearance";
import { choiceKey } from "./features/picker/choice";
import { styleOf } from "./format/outputs";
import { PickerHeader } from "./features/picker/PickerHeader";
import { PickerScroll } from "./features/picker/PickerScroll";
import { Notices } from "./features/picker/Notices";
import { PickerPageToken } from "./features/picker/PickerPageToken";
import { PickerResults } from "./features/picker/PickerResults";
import { PickerCitationShelf } from "./features/picker/PickerCitationShelf";
import { PickerStatusFooter } from "./features/picker/PickerStatusFooter";
import { PickerNotices, PickerOverlays } from "./features/picker/PickerOverlays";
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
import { usePickerMove } from "./features/picker/usePickerMove";
import { useAltHeld } from "./features/picker/useAltHeld";
import { HeldKeys } from "./ui/HeldKeys";
import { useNativeRequests } from "./features/picker/useNativeRequests";
import { usePickerLifecycle } from "./features/picker/usePickerLifecycle";
import { useResultScroll } from "./features/picker/useResultScroll";
import { useCitationFeedback } from "./features/picker/useCitationFeedback";
import { useCitationRows } from "./features/picker/useCitationRows";
import { useProjectBibliography } from "./features/picker/useProjectBibliography";
import { ProjectCheckNotice } from "./features/picker/ProjectContextStatus";
import { useEditorRequest } from "./features/picker/useEditorRequest";
import { usePickerSource } from "./features/picker/usePickerSource";
import { usePickerActions } from "./features/picker/usePickerActions";
import { useCitationInspector } from "./features/picker/useCitationInspector";
import { usePickerKeys } from "./features/picker/usePickerKeys";
import { useCommandSearch } from "./features/picker/useCommandSearch";
import { COMMAND_PREFIX } from "./features/picker/commands";
import { useWindowSizeStatus } from "./features/picker/useWindowSizeStatus";
import type { Copied } from "./api";
import { usePickerDelights } from "./features/picker/usePickerDelights";
import { useEmptyFeedback, useZoomFeedback } from "./ui/Delights";
import { useInterfaceMessages } from "./i18n";

// A stable provider lets component tests use synthetic data without IPC.
// Formatted citations are offered only when the provider can supply them.
export type PickerApi = Pick<typeof nativeApi,
  "chooseLibrary" | "useZotero" | "libraryStatus" | "reloadLibrary" | "previewFileCitation" | "copyFileCitation" | "pickerPreferences" | "setPickerPreferences" |
  "hidePicker" | "pickerPresentation" | "pickerVisible" | "setPinned" | "onLibraryChanged" | "onPickerShown" | "onPickerHidden" | "quit" | "search" | "shortcutStatus"> &
  Partial<Pick<typeof nativeApi, "pickerMoving" | "onLibraryFileDropped" | "onLibraryFileRefused" | "onFileDrag" | "delightView" | "onPickerPreparing" | "recentReferences" | "openLibrarySource" | "librarySource" | "useSampleLibrary" | "useZoteroLibraries" | "zoteroGroups" | "zoteroProfiles" | "useZoteroProfile" | "checkZoteroSharing" | "pickerDestination" | "writingProjects" | "editWritingProject" | "projectStatus" | "retryProject" | "chooseProject" | "stopProject" | "copyProjectEntry" | "onProjectChanged" | "onWritingProjectsChanged" | "onProjectNotSaved" | "rememberAppOutput" | "onWindowSizeSaved" | "readWindowPositionUndo" | "undoWindowPosition" | "dismissWindowPositionUndo" | "onWindowReset" | "findZotero" | "findSources" | "openFoundLibrary" | "rowContext" | "showCopiedNotice" | "loginItem" | "setLoginItem" | "onUpdateProgress" | "updateChecks" | "setUpdateChecks" | "onUpdateChecks" | "updateAvailable" | "onUpdateAvailable" | "updatePickerActivity" | "checkForUpdates" | "onShortcutStatus" | "openSettings" | "onPreferencesChanged" | "takeSearchRequest" | "onSearchRequested" | "alignmentTick" | "resetWindowPosition" | "editorRequest" | "onEditorRequest" | "sessionReferences" | "editSessionReferences" | "sessionBibliography" | "copySessionBibliography" | "exportSessionBibliography">>;

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
  const { density, quitAfterSaving: quitPreferences } = preferences;
  const deliveryTarget = useDeliveryTarget(api);
  const destination = useDestination(deliveryTarget.api, {
    fallback: preferences.format, chooseDefault: preferences.chooseFormat, report,
    onProjectContext: (context) => send({ type: "project-observed", ...context }),
  });
  const { format } = destination;
  const quitAfterSaving = () => destination.quitAfterSaving(quitPreferences);
  const libraryState = usePickerLibrary(api, report, cancelCopy);
  const { library, settledLibrary, libraryVersion, refreshSnapshot, choosingLibrary, welcomeShown, libraryChoices, showCitations } = libraryState;
  const project = useProjectBibliography(api, report, perform);
  const editor = useEditorRequest(api);
  // A waiting editor's format replaces the picker's, so the same choice is asked again.
  // A changed project bibliography asks the same questions of a new answer.
  const checkVersion = libraryVersion + editor.version + project.version;
  const delights = usePickerDelights(api, state.presentation, library, preferences.smallSurprises);
  useEffect(() => { if (choosingLibrary) delights.dismiss(); }, [choosingLibrary, delights.dismiss]);
  const recents = useRecentReferences(api.recentReferences, destination.appId, state.session, api.onPreferencesChanged, () => send({ type: "forget-last" }));
  const searchState = useLibrarySearch({
    // ">" asks for commands, never the library.
    search: api.search, emptySearch: recents.load, query: query.startsWith(COMMAND_PREFIX) ? "" : query, interpretLocators, library: settledLibrary, statusFailed: failures.status ?? false,
    libraryVersion: checkVersion, sessionId: state.session, isCurrentSession, setActive, refreshSnapshot,
  });
  const { queryKey, results, sameSnapshot, ready, interpretation, list, error, pending: searchPending } = searchState;
  // biome-ignore lint/correctness/useExhaustiveDependencies: new results are the trigger: the pointer must move again before it counts.
  useEffect(resetPointer, [results, resetPointer]);
  // What the screen shows: the selection, otherwise the shown pasted list or
  // highlighted result. It stays put while a newer search runs.
  const shownChoice = !showCitations ? [] : selected.size > 0 ? [...selected.values()] : !sameSnapshot ? [] : list ? results
    : results.slice(active, active + 1).map((r) => ({ ...r, locator: interpretation?.locator }));
  // What may be copied: the same, once the shown results answer the current query.
  const chosen = selected.size > 0 || ready ? shownChoice : [];
  const output = useCitationOutput({
    chosen: shownChoice, library: settledLibrary, libraryVersion: checkVersion, sessionId: state.session, opened: state.opened, isCurrentSession, format, style: styleOf(format),
    previewFileCitation: api.previewFileCitation, refreshSnapshot, projectContext: destination.contextVersion, contextReady: destination.ready && !project.reading,
  });
  const { formatted, shown, previewProblem, displayedOutput } = output;
  const checkedPreview = shown && typeof shown === "object" ? shown : null;
  const projectRefusal = checkedPreview?.project_refusal ?? null;
  // A file that can't be read is advice beside a key citation, never a gate.
  const projectProblem = checkedPreview && styleOf(format) === null && (project.status?.state === "failed" || project.status?.state === "damaged")
    ? pickerText.projectDetail(project.status) : null;
  const contextNotice = projectRefusal ?? projectProblem ? `${projectRefusal ?? projectProblem}:${destination.projectVersion()}` : null;
  const citationProblem = displayedOutput === "failed" ? previewProblem ?? pickerText.previewError : null;
  const menus = usePickerMenus(api, operations, libraryState, cancelCopy);
  const { input, formatButtonRef, currentPin, pinned, setFormatOpen, focusHome } = menus;
  useUpdateOfferActivity(api, state, menus, choosingLibrary);
  const { searchFocused, setSearchFocused } = usePickerSearchFocus(input);
  const snapshotRevision = library?.revision;
  const highlightedChoice = choiceKey(results.slice(active, active + 1)
    .map((r) => ({ ...r, locator: interpretation?.locator })));
  const defaultChoice = list && ready ? choiceKey(results) : highlightedChoice;
  const { requestCopy, chooseFormat, onFormatOpenChange } = useCitationCopy({
    api, session, state, destination, currentPin, formatButtonRef, refreshSnapshot, report, setFormatOpen,
    view: {
      queryKey, format, snapshotRevision, defaultChoice, chosen, shown,
      // The citation stays in view through a reload, but Enter waits for its end.
      searchPending, ready: preferences.ready && library?.state !== "reloading",
      remembersLast: preferences.recentCitations === true,
    },
  });
  useLibrarySetup(library, setFailures, send, focusHome);
  const { clearNotice } = project;
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new search is the trigger: the notice was about the row before it.
  useEffect(clearNotice, [queryKey, clearNotice]);
  const { read: readDestination } = destination;
  // The editor takes the citation itself, so no app question or paste applies.
  // biome-ignore lint/correctness/useExhaustiveDependencies: each request and end is the trigger.
  useEffect(() => { if (editor.version > 0) readDestination(true, false); }, [editor.version]);
  const { altHeld, setAltHeld } = useAltHeld();
  useNativeRequests(api, query, setQuery, input, state.selected.size);
  usePickerLifecycle({ api, pickerSession, menus, destination, preferences, resetPointer, readShortcut, report, setAltHeld });
  usePickerMove(api, pickerSession.session);
  const { resultArea, followKeyboard } = useResultScroll({
    input, active, selectAll: state.selectAll, pointer, selectedSize: selected.size, citationProblem, formatted, format, searchFocused,
  });
  useCitationFeedback(copied, preview, send);
  const rows = useCitationRows({
    api, pickerSession, searchState, chosenKey: output.chosenKey, chosen, showCitations, format, snapshotRevision,
    focusHome, pointer, requestCopy, report,
  });
  const nothingMatched = Boolean(query) && searchState.resultsQuery === queryKey && results.length === 0 && !error;
  const scope = useZoteroScope({
    library, libraryVersion, menuOpen: menus.libraryMenuOpen, nothingMatched,
    zoteroGroups: api.zoteroGroups, chooseZoteroLibraries: api.useZoteroLibraries, report, perform,
    focusSearch: focusHome,
  });
  const source = usePickerSource({ api, pickerSession, libraryState, menus, results: sameSnapshot ? results : [], altHeld, perform, report });
  const inspector = useCitationInspector();
  // A failure stays beside the fields that can correct it, in one place, also
  // when the panel takes the list's place.
  const listProblem = inspector.panel ? null : citationProblem;
  const actions = usePickerActions({
    api, pickerSession, menus, libraryState, searchState, destination, shown, inspector,
    // The footer names what Enter copies now, or after the running search.
    offered: chosen.length > 0 || searchPending ? shownChoice : [], canPaste: deliveryTarget.canPaste,
    shortcut, perform, dismissChooser: source.dismissChooser, toggle: rows.toggle,
  });
  const commandKeys = useRef<((event: KeyboardEvent) => boolean) | null>(null);
  const keys = usePickerKeys({
    api, pickerSession, menus, source, results, showCitations, copyChosen: rows.copyChosen, actions, pointer, followKeyboard,
    quitAfterSaving, commandKeys,
    onEmptyEnter: () => {
      if (chosen.length > 0 || searchPending || state.pendingCopy) return;
      delights.dismiss();
      emptyFeedback.refuse(text(library?.state === "loading" || library?.state === "reloading" ? "delight-loading" : !library?.has_snapshot ? "delight-no-library" : "delight-empty"));
    },
  });
  useWindowSizeStatus(api.onWindowSizeSaved, report);
  // ">" in the search lists commands by name (docs/design/picker.md, Command search).
  const commands = useCommandSearch({
    query: showCitations ? query : "", run: keys.run, setQuery,
    source: api.openLibrarySource && source.sourceAvailable ? source.sourceAction.label : null, changeLast: actions.canChangeLast,
  });
  // Peek (⌘Y): the highlighted reference's full details under its row, as Quick Look.
  const [peeked, setPeeked] = useState<string | null>(null);
  const peekRecord = !commands.mode && showCitations ? results[active] : undefined;
  const peekOpen = Boolean(peekRecord && peeked === idKey(peekRecord.id));
  const peekAnchor = useRef<Element | null>(null);
  peekAnchor.current = peekOpen ? document.getElementById(`result-${active}`) : null;
  commandKeys.current = (event) => {
    if (commands.keys(event)) return true;
    if (!matchesAccelerator(event, keysFor("peek"), isMac)) return false;
    event.preventDefault();
    setPeeked(peekOpen || !peekRecord ? null : idKey(peekRecord.id));
    return true;
  };

  return (
    <main
      data-selection={showCitations && selected.size > 0 || undefined}
      onFocus={(e) => setSearchFocused(e.target === input.current)}
      onBlur={() => setSearchFocused(false)}
      onScrollCapture={pointer.resetHover}
    >
      <PickerHeader
        hidden={welcomeShown} pinned={pinned} onTogglePin={() => { keys.run("pin"); focusHome(); }}
        onResetPosition={api.resetWindowPosition}
        inputRef={input} query={query}
        pageToken={<PickerPageToken
          usingLocator={actions.usingLocator} prepared={actions.prepared} canApplyLocator={actions.canApplyLocator}
          changeLocator={rows.changeLocator} send={send} focusHome={focusHome} />}
        onQueryChange={(value) => { delights.dismiss(); pointer.resetHover(); setQuery(value); }}
        expanded={results.length > 0 && showCitations}
        activeDescendant={commands.mode ? commands.list.length ? `command-${commands.active}` : undefined : showCitations && results[active] ? `result-${active}` : undefined}
        format={format} formatButtonRef={formatButtonRef}
        app={destination.app} formatSource={destination.source} formatPick={destination.pick}
        writingProject={destination.writingProject} onWritingProjects={api.writingProjects && !editor.label ? menus.openProjects : undefined}
        onFormatOpenChange={onFormatOpenChange}
        pasteChoices={destination.pasteChoices}
        onPasteChange={(paste) => { const asked = state.pendingCopy?.question === "open"; send({ type: "question", answer: "cancel" }); destination.choosePaste(paste, asked); }}
        formatDisabled={!destination.ready || destination.saving}
        showFormat={!libraryChoices && (library === null || Boolean(library?.has_snapshot))}
        styles onFormatChange={chooseFormat}
      />
      <PickerScroll onFileDrag={api.onFileDrag} presentation={state.visibility} citationProblem={listProblem} citationProblemIdentity={listProblem ? formatted?.request : undefined}
        contextNotice={contextNotice} contextNoticeIdentity={projectRefusal ? formatted?.request : undefined}>
        {shortcut?.error && <div className="notices">
          <p role="status">
            {pickerText.shortcutError(shortcut.registered, shortcut.shortcut, shortcut.error)}
            {shortcut.desktop && !welcomeShown && ` ${shortcut.desktop.guidance}`}
          </p>
        </div>}
        <ProjectCheckNotice refusal={projectRefusal} problem={projectProblem}
          onRetry={project.retry ? () => { cancelCopy(); project.retry?.(); } : undefined}
          onLocate={project.choose ? () => { cancelCopy(); project.choose?.(); } : undefined}
          disabled={!destination.ready || destination.saving || !ready || searchPending} />
        <Notices projectNotice={destination.readProblem ?? project.notice} editorNotice={editor.label} densityAnnouncement={preferences.densityAnnouncement} library={library} failures={{ ...failures, recentHistory: recents.forgetFailed }}
          error={error} copyError={inspector.panel ? null : copyError} copied={copied} missing={ready ? list?.missing : undefined}
          citationProblem={listProblem} confirmation={state.copyNotice}
          onDismissCopied={() => send({ type: "dismiss" })} />
        <PickerResults
          api={api} libraryState={libraryState} searchState={searchState} state={state} source={source} scope={scope}
          shortcut={shortcut} perform={perform} focusHome={focusHome} pointer={pointer} rows={rows}
          density={density} resultArea={resultArea} setQuery={setQuery} peck={delights.peck}
          project={styleOf(format) === null ? project.status : null} commands={commands.mode ? commands : null} />
      </PickerScroll>
      {/* Keep the selection live region mounted when choices remove citation controls. */}
      <PickerCitationShelf
        state={state} send={send} showCitations={showCitations && !commands.mode} results={results} output={output} chosen={shownChoice}
        changeReference={rows.changeReference} copyChosen={rows.copyChosen} libraryOf={scope.libraryOf} format={format} input={input} sourceState={source} inspector={inspector} />
      <PickerStatusFooter
        copyProblem={projectRefusal}
        api={api} library={library} scope={scope} sourceState={source} menus={menus} actions={actions} run={keys.run}
        copyChosen={rows.copyChosen} destination={destination} recents={recents} preferences={preferences}
        operations={operations} altHeld={altHeld} notices={<PickerNotices state={state} send={send} menus={menus} updateProgress={updateProgress} remark={zoomFeedback ?? emptyFeedback.feedback ?? delights.feedback} passive={!zoomFeedback && !emptyFeedback.feedback && delights.passive} onRemarkPassed={() => { if (zoomFeedback) return; if (emptyFeedback.feedback) emptyFeedback.clear(); else delights.dismiss(); }} />} emptyAction={emptyFeedback.shaking} lastOutcome={state.lastOutcome}
        project={project} highlighted={ready && styleOf(format) === null ? results[active] : undefined}
        cancelCopy={cancelCopy} command={commands.footer} />
      <PickerOverlays api={{ ...api, editWritingProject: destination.editProject }} format={format} style={styleOf(format)} menus={menus} bindings={actions.bindings} />
      <HeldKeys />
      {peekRecord && <PeekCard triggerRef={peekAnchor} open={peekOpen} onClose={() => setPeeked(null)}
        label={peekText.label(peekRecord.title || peekRecord.key)} title={peekRecord.title || peekRecord.key} byline={peekRecord.authors}
        facts={[
          ...peekRecord.container ? [{ label: peekText.container, value: peekRecord.container }] : [],
          { label: peekText.year, value: peekRecord.year ? String(peekRecord.year) : pickerText.noDate },
          ...rowText.itemType(peekRecord.item_type) ? [{ label: peekText.type, value: rowText.itemType(peekRecord.item_type)! }] : [],
          { label: peekText.key, value: peekRecord.key, mono: true },
        ]} />}
    </main>
  );
}
