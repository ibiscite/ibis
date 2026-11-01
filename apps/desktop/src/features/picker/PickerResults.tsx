import type { RefObject } from "react";
import type { PickerApi } from "../../App";
import { LibrarySetup, SampleLibraryExit } from "../../LibrarySetup";
import { pickerText } from "../../messages";
import { ResultList } from "./ResultList";
import { EmptyState } from "./EmptyState";
import type { usePickerLibrary } from "./usePickerLibrary";
import type { useLibrarySearch } from "./useLibrarySearch";
import type { usePickerSession } from "./usePickerSession";
import type { usePickerSource } from "./usePickerSource";
import type { useZoteroScope } from "./useZoteroScope";
import type { useShortcutStatus } from "./useShortcutStatus";
import type { useOperations } from "./useOperations";
import type { usePointerSelection } from "../../pointerSelection";
import type { useCitationRows } from "./useCitationRows";
import type { Density } from "../../api";

const noLibrary = () => null;

export function PickerResults({
  api, libraryState, searchState, state, source, scope, shortcut, perform, input, pointer, rows, density,
  resultArea, setQuery,
}: {
  setQuery: (value: string) => void;
  api: PickerApi;
  libraryState: ReturnType<typeof usePickerLibrary>;
  searchState: ReturnType<typeof useLibrarySearch>;
  state: ReturnType<typeof usePickerSession>["state"];
  source: ReturnType<typeof usePickerSource>;
  scope: ReturnType<typeof useZoteroScope>;
  shortcut: ReturnType<typeof useShortcutStatus>["shortcut"];
  perform: ReturnType<typeof useOperations>["perform"];
  input: RefObject<HTMLInputElement | null>;
  pointer: ReturnType<typeof usePointerSelection>;
  rows: Pick<ReturnType<typeof useCitationRows>, "onChoose" | "onContext" | "onToggle">;
  density: Density;
  resultArea: RefObject<HTMLDivElement | null>;
}) {
  const { choosingLibrary, setupApi, setWelcomeShown, library, libraryChoices, busy, blocked, showCitations } = libraryState;
  const { query, active, selected } = state;
  const { results, error, list, announcement } = searchState;
  const { narrowed, searchAllLibraries, severalLibraries, libraryOf } = scope;
  const { sourceChoice, performSourceChoice } = source;
  const { chooseLibrary, reloadLibrary } = api;
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

  return <div className="results-area scroll-area" ref={resultArea}>
    {library?.has_snapshot && results.length > 0 && !results.some((r) => r.recent) && !busy && showCitations && <div className="results-heading" aria-hidden="true">
      <span>{list ? pickerText.pastedList : results.some((r) => r.partial) ? pickerText.closestMatches : query ? pickerText.bestMatches : pickerText.references}</span><bdi>{pickerText.resultCount(results.length)}</bdi>
    </div>}
    <p className="visually-hidden" role="status" aria-label={pickerText.searchResults} aria-live="polite" aria-atomic="true">
      {showCitations ? announcement : ""}
    </p>
    {busy && emptyState}
    {showCitations && <ResultList results={results} active={active} selected={selected} density={density} hoverAllowed={pointer.hoverAllowed}
      libraryOf={severalLibraries ? libraryOf : noLibrary} onMouseMove={pointer.onMouseMove}
      onChoose={rows.onChoose} onContext={rows.onContext} onToggle={rows.onToggle} />}
    {!busy && emptyState}
    {library?.sample && <SampleLibraryExit useSampleLibrary={api.useSampleLibrary} perform={perform} />}
  </div>;
}
