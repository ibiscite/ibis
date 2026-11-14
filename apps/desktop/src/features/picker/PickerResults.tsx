import { Status } from "../../ui/Status";
import type * as nativeApi from "../../api";
import type { RefObject } from "react";
import { LibrarySetup, SampleLibraryExit } from "../../LibrarySetup";
import { say } from "../../i18n";
import { keysFor } from "../../keys";
import { FrameHeading, FrameNotices } from "../../ui/Frame";
import { ResultList } from "./ResultList";
import { EmptyState, useZoteroRecovery } from "./EmptyState";
import { CommandList } from "./CommandList";
import type { PickerCommandEntry } from "./commands";
import type { usePickerLibrary } from "./usePickerLibrary";
import type { useLibrarySearch } from "./useLibrarySearch";
import type { usePickerSession } from "./usePickerSession";
import type { usePickerSource } from "./usePickerSource";
import type { useZoteroScope } from "./useZoteroScope";
import type { useShortcutStatus } from "./useShortcutStatus";
import type { useOperations } from "./useOperations";
import type { usePointerSelection } from "../../pointerSelection";
import type { useCitationRows } from "./useCitationRows";
import type { Density, ProjectStatus } from "../../api";

const noLibrary = () => null;

export function PickerResults({
  api, libraryState, searchState, state, source, scope, shortcut, perform, focusHome, pointer, rows, density,
  resultArea, setQuery, peck, project, commands, numbered,
}: {
  /** The rows ⌘1–9 picks (useShownRows). */
  numbered?: { first: number; count: number };
  /** ">" in the search: the commands, in place of the references. */
  commands?: { list: readonly PickerCommandEntry[]; active: number; run: (command: PickerCommandEntry) => void } | null;
  /** The bibliography rows are marked against; null for formatted output. */
  project: ProjectStatus | null;
  setQuery: (value: string) => void;
  peck?: boolean;
  api: typeof nativeApi;
  libraryState: ReturnType<typeof usePickerLibrary>;
  searchState: ReturnType<typeof useLibrarySearch>;
  state: ReturnType<typeof usePickerSession>["state"];
  source: ReturnType<typeof usePickerSource>;
  scope: ReturnType<typeof useZoteroScope>;
  shortcut: ReturnType<typeof useShortcutStatus>["shortcut"];
  perform: ReturnType<typeof useOperations>["perform"];
  focusHome: () => void;
  pointer: ReturnType<typeof usePointerSelection>;
  rows: Pick<ReturnType<typeof useCitationRows>, "onChoose" | "onContext" | "onToggle">;
  density: Density;
  resultArea: RefObject<HTMLDivElement | null>;
}) {
  const { choosingLibrary, setupApi, setWelcomeShown, library, libraryChoices, busy, blocked, showCitations } = libraryState;
  const { active, selected } = state;
  // The shown results and empty message answer this query until the next reply.
  const { results, error, list, unfiltered, announcement, answered: query } = searchState;
  const { narrowed, searchAllLibraries, severalLibraries, libraryOf } = scope;
  const { sourceChoice, performSourceChoice } = source;
  const { chooseLibrary, reloadLibrary } = api;
  // EmptyState is absent when the retained library has matching rows (or a
  // command is shown). Recovery belongs to the visible picker in those cases.
  useZoteroRecovery(library?.has_snapshot ? library : null, api.checkZoteroSharing, !choosingLibrary);
  // A failed refresh keeps the older references searchable, so a search that
  // finds nothing there still says so; only a library with nothing to search
  // hands the middle area to its recovery screen.
  const searchable = !library?.error || blocked || Boolean(library?.has_snapshot);
  const emptyState = choosingLibrary ? <LibrarySetup api={setupApi} perform={performSourceChoice} autoFocus
    shortcut={shortcut} onWelcomeChange={setWelcomeShown} /> : (library !== undefined && (libraryChoices || busy || results.length === 0 && !error && searchable) ? (
    <EmptyState library={library} query={query} narrowed={narrowed} peck={peck}
      unfiltered={unfiltered} missing={Boolean(list?.missing.length)}
      onRemoveFilter={(plain) => { setQuery(plain); focusHome(); }}
      source={source.sourceInChoices ? sourceChoice : undefined}
      // With no reference highlighted, Zotero's own action is opening Zotero (decision 0047).
      fallback={source.sourceAvailable ? source.sourceAction
        : library?.zotero ? { label: say("picker-open-zotero"), keys: keysFor("source"), open: () => source.openSource(true) } : undefined}
      onChooseLibrary={() => void perform("choose", chooseLibrary)}
      onClearSearch={() => { setQuery(""); focusHome(); }}
      onSearchAll={searchAllLibraries}
      onReload={() => { focusHome(); void perform("reload", reloadLibrary); }}
      zotero={{ listProfiles: api.zoteroProfiles, checkSharing: api.checkZoteroSharing,
        chooseProfile: api.useZoteroProfile && ((name) => { void perform("zotero", async () => api.useZoteroProfile!(name)); }) }}
      setup={
        (focus, empty) => <LibrarySetup api={setupApi} perform={perform} autoFocus={focus} empty={empty} shortcut={shortcut} onWelcomeChange={setWelcomeShown} />
      } />
  ) : null);

  if (commands) return <div className="results-area scroll-area" ref={resultArea}>
    <CommandList commands={commands.list} active={commands.active} hoverAllowed={pointer.hoverAllowed} onRun={commands.run} />
  </div>;
  const recent = results[0]?.recent;
  // A reloading library keeps its list and heading line: its notices stay in that line.
  const listShown = showCitations && results.length > 0;
  return <>{emptyState && !listShown && <FrameNotices />}<div className="results-area scroll-area" ref={resultArea}>
    {/* The list's heading sits in the frame's line, where a notice can stand in for it. */}
    {library?.has_snapshot && results.length > 0 && library.state !== "loading" && showCitations && <FrameHeading>{recent
      ? <div className="ui-list-heading"><h2 id={`recent-${recent}`}>{recent === "cited" ? "Cited here recently" : "Recently added"}</h2></div>
      : <div className="ui-list-heading" aria-hidden="true">
        <span>{list ? say("picker-pasted-list") : results.some((r) => r.partial) ? say("picker-closest-matches") : query ? say("picker-best-matches") : say("picker-references")}</span><bdi>{say("picker-result-count", { count: results.length })}</bdi>
      </div>}</FrameHeading>}
    <Status as="p" className="visually-hidden" label={say("picker-search-results")}>
      {showCitations ? announcement : ""}
    </Status>
    {busy && emptyState}
    {showCitations && <ResultList results={results} active={active} selected={selected} density={density} hoverAllowed={pointer.hoverAllowed}
      libraryOf={severalLibraries ? libraryOf : noLibrary} project={project} numbered={numbered} onMouseMove={pointer.onMouseMove}
      onChoose={rows.onChoose} onContext={rows.onContext} onToggle={rows.onToggle} />}
    {!busy && emptyState}
    {library?.sample && !choosingLibrary && <SampleLibraryExit useSampleLibrary={api.useSampleLibrary} perform={perform} />}
  </div></>;
}
