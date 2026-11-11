import type { ReactNode } from "react";
import { say } from "../../i18n";
import { zoteroLibraryNames } from "../../LibraryStatusLine";
import type { PickerApi } from "../../App";
import type { LibraryStatus, SearchResult } from "../../api";
import { PickerFooter } from "./PickerFooter";
import { PickerLibraryStatus } from "./PickerLibraryStatus";
import { PickerMenu } from "./PickerMenu";
import { libraryOperations, OPERATION_FAILURES } from "./Notices";
import { closeNearestLayer } from "../../ui/layers";
import type { useZoteroScope } from "./useZoteroScope";
import type { usePickerSource } from "./usePickerSource";
import type { usePickerMenus } from "./usePickerMenus";
import type { usePickerActions } from "./usePickerActions";
import type { usePickerKeys } from "./usePickerKeys";
import type { useCitationRows } from "./useCitationRows";
import type { useDestination } from "./useDestination";
import type { useRecentReferences } from "./useRecentReferences";
import type { usePickerPreferences } from "./usePickerPreferences";
import type { useOperations } from "./useOperations";
import type { useProjectBibliography } from "./useProjectBibliography";
import { libraryMessages } from "../../libraryWords";

export function PickerStatusFooter({
  api, library, scope, sourceState, menus, actions, run, copyChosen, recents,
  operations, altHeld, lastOutcome, emptyAction, notices, project, highlighted, copyProblem, command, peek,
}: {
  /** The highlighted reference's details (⌘Y), when there is one. */
  peek?: (() => void) | null;
  project: ReturnType<typeof useProjectBibliography>;
  /** The active row of a key output, the one a missing entry can be copied for. */
  highlighted?: SearchResult;
  api: PickerApi;
  library: LibraryStatus | null | undefined;
  scope: ReturnType<typeof useZoteroScope>;
  sourceState: ReturnType<typeof usePickerSource>;
  menus: ReturnType<typeof usePickerMenus>;
  actions: ReturnType<typeof usePickerActions>;
  run: ReturnType<typeof usePickerKeys>["run"];
  copyChosen: ReturnType<typeof useCitationRows>["copyChosen"];
  destination: ReturnType<typeof useDestination>;
  recents: ReturnType<typeof useRecentReferences>;
  preferences: ReturnType<typeof usePickerPreferences>;
  operations: ReturnType<typeof useOperations>;
  altHeld: boolean;
  lastOutcome: string | null;
  emptyAction?: boolean;
  notices?: ReactNode;
  copyProblem?: string | null;
  /** ">" in the search: Enter runs this, so the footer says so. */
  command?: { label: string | null; run: () => void } | null;
}) {
  const { useZoteroLibraries: chooseZoteroLibraries, reloadLibrary, useZotero, openSettings } = api;
  const { groupName, offeredGroups, isChosen, chosenLibraries, toggleLibrary, searchAllLibraries } = scope;
  const { libraryMenuOpen, openLibrary, closeLibrary, input, menuOpen, openMore, closeMore, moreButton, pinned, focusHome, runFromMenu } = menus;
  const { sourceAction, sourceOpen, source, chooseSources, chooseFile, openSource } = sourceState;
  const { failures } = operations;
  const { toggleHighlighted, hints } = actions;
  // Read when More opens, the only time it is shown there.
  const libraryDetail = menuOpen ? libraryMessages.detail(library, Date.now(), (status) => zoteroLibraryNames(status, groupName)) : null;
  return <PickerFooter library={library} groupName={groupName} emptyAction={emptyAction} copyProblem={copyProblem}
    status={<PickerLibraryStatus library={library} groupName={groupName} notice={notices} isOpen={libraryMenuOpen}
      onOpenChange={(open) => (open ? openLibrary() : closeLibrary())}
      searchRef={input} sourceAction={sourceAction} sourceFailure={sourceOpen.failure !== null}
      statusIcon={source.status_icon ?? null} hideAction={sourceState.libraryChoices}
      rowAction={menuOpen && api.openLibrarySource && sourceState.sourceAvailable ? sourceAction.label : undefined}
      zotero={library?.zotero && chooseZoteroLibraries ? {
        offered: ["user" as const, ...offeredGroups.map((group) => ({ group }))], isChosen, chosenCount: chosenLibraries.length,
        name: (choice) => typeof choice === "object" ? groupName(choice.group) : say("picker-my-library"), toggle: toggleLibrary, all: searchAllLibraries,
      } : null}
      actions={{ choose: chooseSources, find: chooseFile, reload: () => runFromMenu("reload", reloadLibrary),
        useZotero: () => runFromMenu("zotero", useZotero), openZotero: () => openSource(true) }}
      listProfiles={api.zoteroProfiles} chooseProfile={api.useZoteroProfile && ((name) => runFromMenu("zotero", async () => api.useZoteroProfile!(name)))}
      groupsFailed={failures.zoteroGroups} failure={sourceOpen.failure?.message ?? [...libraryOperations].filter((operation) => operation !== "zoteroGroups" && failures[operation]).map((operation) => say(OPERATION_FAILURES[operation])).join(" ")} />}

    hints={command ? { ...hints, enter: command.label, shiftEnter: null } : hints}
    altHeld={altHeld} actions={{ copy: command ? command.run : copyChosen, add: toggleHighlighted, escape: () => closeNearestLayer() }}
    more={
      <PickerMenu isOpen={menuOpen} onOpenChange={(open) => (open ? openMore() : closeMore())}
        buttonRef={moreButton} searchRef={input} libraryDetail={libraryDetail}
        source={api.openLibrarySource && sourceState.sourceAvailable ? sourceAction : undefined}
        lastOutcome={lastOutcome}
        project={{ status: project.status, choose: project.choose, stop: project.stop,
          copyEntry: highlighted?.project?.state === "missing" && highlighted.project.generation === project.status?.generation
            ? () => { project.copyEntry(highlighted); focusHome(); } : null }}
        pinned={pinned} keepOrder={actions.keepOrder}
        actions={{
          pin: () => run("pin"),
          keys: () => run("keys"),
          format: () => run("format"),
          chooseSource: () => run("chooseSource"),
          revealSource: api.openLibrarySource && sourceState.sourceAvailable ? () => run("revealSource") : null,
          peek,
          changeLast: actions.canChangeLast ? () => run("changeLast") : null,
          settings: openSettings ? () => run("settings") : null,
          sessionReferences: api.sessionReferences ? menus.openSession : null,
          forgetRecent: recents.forget ? () => { recents.forget!(); focusHome(); } : null,
          quit: () => run("quit"),
        }} />
    } />;
}
