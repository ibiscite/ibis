import * as nativeApi from "./api";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { idKey, asTyped } from "./features/picker/choice";
import { keysFor, matchesAccelerator } from "./keys";
import { isMac } from "./platform";
import { PeekCard } from "./ui/PeekCard";

import { fullStop, say } from "./i18n";
import { usePointerSelection } from "./pointerSelection";
import { usePickerAppearance } from "./ui/appearance";
import { isStyle, styleOf } from "./format/outputs";
import { PickerHeader } from "./features/picker/PickerHeader";
import type { FormatPreview } from "./features/picker/FormatMenu";
import { previewOf } from "./format/outputs";
import { PickerScroll } from "./features/picker/PickerScroll";
import { notices } from "./features/picker/Notices";
import { Frame, LineNotice } from "./ui/Frame";
import { PickerPageToken } from "./features/picker/PickerPageToken";
import { PickerResults } from "./features/picker/PickerResults";
import { PickerCitationShelf } from "./features/picker/PickerCitationShelf";
import type { CitationLineActions } from "./features/picker/CitationShelf";
import { PickerStatusFooter } from "./features/picker/PickerStatusFooter";
import { PickerNotices, PickerOverlays } from "./features/picker/PickerOverlays";
import { useOperations } from "./features/picker/useOperations";
import { useShortcutStatus } from "./features/picker/useShortcutStatus";
import { usePickerPreferences } from "./features/picker/usePickerPreferences";
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
import { useCitationCopy } from "./features/picker/useCitationCopy";
import { usePickerMove } from "./features/picker/usePickerMove";
import { useAltHeld } from "./features/picker/useAltHeld";
import { HeldKeys } from "./ui/HeldKeys";
import { useNativeRequests } from "./features/picker/useNativeRequests";
import { usePickerLifecycle } from "./features/picker/usePickerLifecycle";
import { useResultScroll } from "./features/picker/useResultScroll";
import { useShownRows } from "./features/picker/useShownRows";
import { useCitationRows } from "./features/picker/useCitationRows";
import { bibliographyDetail, useBibliography } from "./features/picker/useBibliography";
import { bibliographyCheckNotice } from "./features/picker/BibliographyStatus";
import { useEditorRequest } from "./features/picker/useEditorRequest";
import { usePickerSource } from "./features/picker/usePickerSource";
import { usePickerActions } from "./features/picker/usePickerActions";
import { useCitationInspector } from "./features/picker/useCitationInspector";
import { useShorthandHint } from "./features/picker/useShorthandHint";
import { usePickerKeys } from "./features/picker/usePickerKeys";
import { useCommandSearch } from "./features/picker/useCommandSearch";
import { COMMAND_PREFIX } from "./features/picker/commands";
import type { Copied } from "./api";
import { usePickerDelights } from "./features/picker/usePickerDelights";
import { useEmptyFeedback, useZoomFeedback } from "./ui/Delights";
import { useInterfaceMessages } from "./i18n";
import { rowWords } from "./features/picker/ResultList";
import { PickerShown } from "./features/picker/pickerShown";

export type PickerPreview = { query?: string; copied?: Copied };

// The picker: search, choose one or more references, copy them as a citation.
export function App({ preview }: { preview?: PickerPreview } = {}) {
  const api = usePickerAppearance(nativeApi);
  const pickerSession = usePickerSession(preview);
  const { session, state, send, setQuery, setActive, cancelCopy } = pickerSession;
  const { query, interpretLocators, active, selected, copied, copyError } = state;
  // The page's one hidden mark, from the session: CSS pauses looping animations
  // with it. Unknown counts as hidden, so nothing runs before the native read.
  useLayoutEffect(() => {
    if (state.shown) delete document.documentElement.dataset.pickerHidden;
    else document.documentElement.dataset.pickerHidden = "";
  }, [state.shown]);
  useLayoutEffect(() => () => { delete document.documentElement.dataset.pickerHidden; }, []);
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
  const destination = useDestination(session, state, api, { fallback: preferences.format, chooseDefault: preferences.chooseFormat, report });
  const { format } = destination;
  const quitAfterSaving = () => quitPreferences(session.afterSaves(2_000));
  const libraryState = usePickerLibrary(api, operations, cancelCopy);
  const { library, settledLibrary, libraryVersion, refreshSnapshot, choosingLibrary, welcomeShown, libraryChoices, showCitations } = libraryState;
  const project = useBibliography(api, report, perform);
  const editor = useEditorRequest(api);
  // A waiting editor's format replaces the picker's, so the same choice is asked again.
  // A changed bibliography asks the same questions of a new answer.
  const checkVersion = libraryVersion + editor.version + project.version;
  const delights = usePickerDelights(api, state.presentation, library, preferences.smallSurprises);
  useEffect(() => { if (choosingLibrary) delights.dismiss(); }, [choosingLibrary, delights.dismiss]);
  const recents = useRecentReferences(api.recentReferences, destination.appId, state.session, api.onPreferencesChanged, () => send({ type: "forget-last" }));
  const searchState = useLibrarySearch({
    // ">" asks for commands, never the library.
    session, state, search: api.search, emptySearch: recents.load, app: destination.appId, query: query.startsWith(COMMAND_PREFIX) ? "" : query, interpretLocators,
    library: settledLibrary, statusFailed: failures.status ?? false, libraryVersion: checkVersion,
  });
  const { queryKey, results, sameSnapshot, ready, interpretation, list, error, pending: searchPending } = searchState;
  // biome-ignore lint/correctness/useExhaustiveDependencies: new results are the trigger: the pointer must move again before it counts.
  useEffect(resetPointer, [results, resetPointer]);
  // What the screen shows: the selection, otherwise the shown pasted list or
  // highlighted result. It stays put while a newer search runs.
  const shownChoice = !showCitations ? [] : selected.size > 0 ? [...selected.values()] : !sameSnapshot ? [] : list ? results
    : results.slice(active, active + 1).map((r) => asTyped(r, interpretation));
  // What may be copied: the same, once the shown results answer the current query.
  const chosen = selected.size > 0 || ready ? shownChoice : [];
  const output = useCitationOutput({
    session, state, chosen: shownChoice, library: settledLibrary, libraryVersion: checkVersion, format, style: styleOf(format),
    previewFileCitation: api.previewFileCitation, writingContext: destination.contextVersion, contextReady: destination.ready && !project.reading,
  });
  const { formatted, shown, previewProblem, displayedOutput } = output;
  const checkedPreview = shown && typeof shown === "object" ? shown : null;
  const projectRefusal = checkedPreview?.project_refusal ?? null;
  // A file that can't be read is advice beside a key citation, never a gate.
  const projectProblem = checkedPreview && styleOf(format) === null && (project.status?.state === "failed" || project.status?.state === "damaged")
    ? bibliographyDetail(project.status) : null;
  const citationProblem = displayedOutput === "failed" ? previewProblem ?? say("picker-preview-error") : null;
  const menus = usePickerMenus(api, operations, libraryState, cancelCopy);
  const { input, formatButtonRef, currentPin, setFormatOpen, focusHome } = menus;
  useUpdateOfferActivity(api, state, menus, choosingLibrary);
  const snapshotRevision = library?.revision;
  const { requestCopy, chooseFormat, onFormatOpenChange } = useCitationCopy({
    api, session, state, destination, currentPin, formatButtonRef, refreshSnapshot, report, setFormatOpen,
  });
  // What the session hears from preferences and the library's lifecycle.
  const statusFailed = failures.status ?? false, remembersLast = preferences.recentCitations === true;
  useLayoutEffect(() => {
    send({ type: "picker-inputs", inputs: { fallback: preferences.format, remembersLast, preferencesReady: preferences.ready, library, settledLibrary,
      statusFailed, showCitations, version: checkVersion } });
  }, [send, preferences.format, remembersLast, preferences.ready, library, settledLibrary, statusFailed, showCitations, checkVersion]);
  // Last of App's layout effects: every input above has reached the session,
  // so an Enter waiting on an engine answer may copy now.
  useLayoutEffect(() => { send({ type: "committed" }); });
  const sample = Boolean(library?.sample);
  const previousSample = useRef(sample);
  useEffect(() => {
    if (previousSample.current === sample) return;
    previousSample.current = sample;
    send({ type: "sample" });
    focusHome();
  }, [sample, send, focusHome]);
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
    input, active, selectAll: state.selectAll, pointer, selectedSize: selected.size, citationProblem, formatted, format,
  });
  // Success is brief; warnings stay available until explicitly dismissed.
  useEffect(() => {
    if (preview || !copied || copied.warnings.length > 0) return;
    const timer = window.setTimeout(() => send({ type: "dismiss", copied }), 2400);
    return () => window.clearTimeout(timer);
  }, [copied, preview, send]);
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
  const inspector = useCitationInspector(cancelCopy);
  const shorthand = useShorthandHint(api, state.presentation, inspector.panel?.kind === "reference");
  // A failure stays beside the fields that can correct it, in one place, also
  // when the panel takes the list's place.
  const actions = usePickerActions({
    api, pickerSession, menus, libraryState, searchState, destination, shown,
    // The footer names what Enter copies now, or after the running search.
    offered: chosen.length > 0 || searchPending ? shownChoice : [], canPaste: destination.canPaste,
    shortcut, perform, dismissChooser: source.dismissChooser, toggle: rows.toggle,
    // A waiting editor's output replaces the picker's.
    writesStyle: editor.label ? editor.style : isStyle(destination.format),
  });
  // The format chooser's preview reads the chosen references' keys and never asks
  // the engine: an engine preview would replace the ticket the shelf copies with.
  // A reference's citation key, never its record ID (the engine's, e.g. Zotero's item key).
  const previewKeys = !editor.label && output.ask ? shownChoice.map((item) => item.key).filter(Boolean).join("\u0000") : "";
  const formatPreview = useCallback<FormatPreview>((other) => previewOf(other, shown && typeof shown === "object" && other === shown.effective_output ? shown : null, previewKeys ? previewKeys.split("\u0000") : []), [shown, previewKeys]);
  const commandKeys = useRef<((event: KeyboardEvent) => boolean) | null>(null);
  // The command list (">") stands in for the results: ⌘1–9 has no row then.
  const commandsShown = useRef(false);
  // The citation line's tokens, for ⌘↓ in Search and the citation's commands in More.
  const citationLine = useRef<CitationLineActions | null>(null);
  // ⌘1–9 picks a row in view, as a double-click on it does, only while the list is shown.
  const numbered = useShownRows(resultArea, results);
  const keys = usePickerKeys({
    api, pickerSession, menus, source, results, showCitations, copyChosen: rows.copyChosen, actions, pointer, followKeyboard,
    quitAfterSaving, commandKeys, citationLine, findOpen: destination.findOpen,
    pick: (number) => {
      const index = numbered.first + number - 1, record = results[index];
      // The list must be drawn: reading a PDF folds it away.
      const drawn = resultArea.current?.checkVisibility?.() ?? true;
      return ready && showCitations && drawn && !commandsShown.current && number <= numbered.count && record ? () => {
        if (destination.findOpen) source.openReference(record.id, false);
        else rows.onChoose(index, record, "copy");
      } : null;
    },
    onEmptyEnter: () => {
      if (chosen.length > 0 || searchPending || state.pendingCopy) return;
      delights.dismiss();
      emptyFeedback.refuse(text(library?.state === "loading" || library?.state === "reloading" ? "delight-loading" : !library?.has_snapshot ? "delight-no-library" : "delight-empty"));
    },
  });
  const { onWindowSizeSaved } = api;
  useEffect(() => {
    let current = true;
    const subscription = onWindowSizeSaved?.((saved) => { if (current) report("windowSize", !saved); });
    void subscription?.catch(() => { if (current) report("windowSize"); });
    return () => { current = false; void subscription?.then((stop) => stop()).catch(() => {}); };
  }, [onWindowSizeSaved, report]);
  // ">" in the search lists commands by name (docs/design/picker.md, Command search).
  const commands = useCommandSearch({
    query: showCitations ? query : "", run: keys.run, setQuery,
    source: source.sourceAvailable ? source.sourceAction.label : null, changeLast: actions.canChangeLast,
  });
  commandsShown.current = commands.mode;
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
    <PickerShown value={state.shown}>
    <Frame
      data-selection={showCitations && selected.size > 0 || undefined}
      onScrollCapture={pointer.resetHover}
      notice={[
        // What passes (Undo, a remark, an update) first: it goes by itself.
        <PickerNotices key="passing" state={state} send={send} menus={menus} updateProgress={updateProgress} remark={zoomFeedback ?? emptyFeedback.feedback ?? shorthand.hint ?? delights.feedback} passive={!zoomFeedback && !emptyFeedback.feedback && !shorthand.hint && delights.passive} onRemarkPassed={() => { if (zoomFeedback) return; if (emptyFeedback.feedback) emptyFeedback.clear(); else if (shorthand.hint) shorthand.dismiss(); else delights.dismiss(); }} />,
        ...notices({ projectNotice: destination.readProblem ?? project.notice, editorNotice: editor.label, library, failures: { ...failures, recentHistory: recents.forgetFailed },
          error, copyError: inspector.panel ? null : copyError, copied, missing: ready ? list?.missing : undefined,
          confirmation: state.copyNotice, onDismissCopied: () => send({ type: "dismiss" }) }),
        bibliographyCheckNotice({ refusal: projectRefusal, problem: projectProblem,
          onRetry: project.retry ? () => { cancelCopy(); project.retry?.(); } : undefined,
          onLocate: project.choose ? () => { cancelCopy(); project.choose?.(); } : undefined,
          disabled: !destination.ready || destination.saving || !ready || searchPending }),
        shortcut?.error && <LineNotice key="shortcut">
          {`${shortcut.registered ? say("picker-shortcut-not-used", { reason: fullStop(shortcut.error) })
            : say("picker-shortcut-unavailable", { shortcut: shortcut.shortcut, reason: fullStop(shortcut.error) })}${shortcut.desktop && !welcomeShown ? ` ${shortcut.desktop.guidance}` : ""}`}
        </LineNotice>,
      ].filter(Boolean)}
      search={<PickerHeader
        hidden={welcomeShown} yours={preferences.format} preview={formatPreview}
        onResetPosition={api.resetWindowPosition}
        inputRef={input} query={query}
        pageToken={<PickerPageToken
          usingLocator={actions.usingLocator} prepared={actions.prepared} canApplyLocator={actions.canApplyLocator}
          changeLocator={rows.changeLocator} send={send} focusHome={focusHome} />}
        onQueryChange={(value) => { delights.dismiss(); pointer.resetHover(); setQuery(value); }}
        expanded={results.length > 0 && showCitations}
        activeDescendant={commands.mode ? commands.list.length ? `command-${commands.active}` : undefined : showCitations && results[active] ? `result-${active}` : undefined}
        format={format} formatLabel={destination.formatLabel} formatButtonRef={formatButtonRef}
        app={destination.app} formatSource={destination.source} formatPick={destination.pick}
        writing={destination.writing} usualFormat={destination.usualFormat} onUseUsual={destination.useUsual} onWritings={!editor.label ? menus.openProjects : undefined}
        onFormatOpenChange={onFormatOpenChange}
        pasteChoices={destination.pasteChoices}
        onPasteChange={(paste) => { const asked = state.pendingCopy?.question === "open"; send({ type: "question", answer: "cancel" }); destination.choosePaste(paste, asked); }}
        formatDisabled={!destination.canChooseFormat}
        showFormat={!libraryChoices && (library === null || Boolean(library?.has_snapshot))}
        styles onFormatChange={chooseFormat}
      />}
    >
      <PickerScroll onFileDrag={api.onFileDrag} presentation={state.visibility}>
        <PickerResults
          api={api} libraryState={libraryState} searchState={searchState} state={state} source={source} scope={scope}
          shortcut={shortcut} perform={perform} focusHome={focusHome} pointer={pointer} rows={{ ...rows, onChoose: (index, reference, action) => {
            if (destination.findOpen && action === "copy" && searchState.ready) source.openReference(reference.id, false);
            else rows.onChoose(index, reference, action);
          } }}
          density={density} resultArea={resultArea} setQuery={setQuery} peck={delights.peck}
          project={styleOf(format) === null ? project.status : null} commands={commands.mode ? commands : null} numbered={numbered} />
      </PickerScroll>
      {/* Keep the selection live region mounted when choices remove citation controls. */}
      <PickerCitationShelf
        state={state} send={send} showCitations={showCitations && !commands.mode} results={results} output={output} chosen={shownChoice}
        changeReference={rows.changeReference} copyChosen={rows.copyChosen} libraryOf={scope.libraryOf} format={format} input={input} inspector={inspector} citationLine={citationLine}
        deliverable={!searchPending && preferences.ready && library?.state !== "reloading" && destination.ready && !destination.saving} />
      <PickerStatusFooter
        copyProblem={projectRefusal}
        api={api} library={library} scope={scope} sourceState={source} menus={menus} actions={actions} run={keys.run} citationLine={citationLine}
        copyChosen={rows.copyChosen} recents={recents}
        operations={operations} altHeld={altHeld}  emptyAction={emptyFeedback.shaking} lastOutcome={state.lastOutcome}
        project={project} highlighted={ready && styleOf(format) === null ? results[active] : undefined}
        command={commands.footer} peek={peekRecord ? () => setPeeked(idKey(peekRecord.id)) : null} />
      <PickerOverlays api={{ ...api, editWriting: destination.editWriting! }} format={format} style={styleOf(format)} menus={menus} bindings={actions.bindings} />
      <HeldKeys />
      {peekRecord && <PeekCard triggerRef={peekAnchor} open={peekOpen} onClose={() => setPeeked(null)}
        label={say("peek-label", { title: peekRecord.title || peekRecord.key })} title={peekRecord.title || peekRecord.key} byline={peekRecord.authors}
        facts={[
          ...peekRecord.container ? [{ label: say("peek-container"), value: peekRecord.container }] : [],
          { label: say("peek-year"), value: peekRecord.year ? String(peekRecord.year) : say("picker-no-date") },
          ...rowWords.itemType(peekRecord.item_type) ? [{ label: say("peek-type"), value: rowWords.itemType(peekRecord.item_type)! }] : [],
          { label: say("peek-key"), value: peekRecord.key, mono: true },
        ]} />}
    </Frame>
    </PickerShown>
  );
}
