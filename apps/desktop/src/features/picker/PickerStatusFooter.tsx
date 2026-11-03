import type { ReactNode } from "react";
import { libraryMessages, operationFailures as FAILURE_MESSAGES, pickerText } from "../../messages";
import { zoteroLibraryNames } from "../../LibraryStatusLine";
import type { PickerApi } from "../../App";
import type { LibraryStatus, SearchResult } from "../../api";
import { PickerFooter } from "./PickerFooter";
import { PickerLibraryStatus } from "./PickerLibraryStatus";
import { PickerMenu } from "./PickerMenu";
import { libraryOperations } from "./Notices";
import type { useZoteroScope } from "./useZoteroScope";
import type { usePickerSource } from "./usePickerSource";
import type { usePickerMenus } from "./usePickerMenus";
import type { usePickerActions } from "./usePickerActions";
import type { useCitationRows } from "./useCitationRows";
import type { useDestination } from "./useDestination";
import type { useRecentReferences } from "./useRecentReferences";
import type { usePickerPreferences } from "./usePickerPreferences";
import type { useOperations } from "./useOperations";
import type { useProjectBibliography } from "./useProjectBibliography";

export function PickerStatusFooter({
  api, library, scope, sourceState, menus, actions, copyChosen, destination, recents, preferences,
  operations, altHeld, lastOutcome, cancelCopy, quitAfterSaving, emptyAction, notices, project, highlighted,
}: {
  project: ReturnType<typeof useProjectBibliography>;
  /** The active row of a key output, the one a missing entry can be copied for. */
  highlighted?: SearchResult;
  api: PickerApi;
  library: LibraryStatus | null | undefined;
  scope: ReturnType<typeof useZoteroScope>;
  sourceState: ReturnType<typeof usePickerSource>;
  menus: ReturnType<typeof usePickerMenus>;
  actions: ReturnType<typeof usePickerActions>;
  copyChosen: ReturnType<typeof useCitationRows>["copyChosen"];
  destination: ReturnType<typeof useDestination>;
  recents: ReturnType<typeof useRecentReferences>;
  preferences: ReturnType<typeof usePickerPreferences>;
  operations: ReturnType<typeof useOperations>;
  altHeld: boolean;
  lastOutcome: string | null;
  cancelCopy: () => void;
  quitAfterSaving: () => Promise<void>;
  emptyAction?: boolean;
  notices?: ReactNode;
}) {
  const { useZoteroLibraries: chooseZoteroLibraries, reloadLibrary, useZotero, openSettings } = api;
  const { groupName, offeredGroups, isChosen, chosenLibraries, toggleLibrary, searchAllLibraries } = scope;
  const { libraryMenuOpen, setMenuOpen, setLibraryMenuOpen, input, menuOpen, openMore, closeMenu, moreButton, pinned, togglePin, openKeys, runFromMenu } = menus;
  const { sourceAction, sourceOpen, source, chooseSources, chooseFile, openSource } = sourceState;
  const { density, changeDensity } = preferences;
  const { failures, report } = operations;
  const { toggleHighlighted, escapeStep, hints } = actions;
  // Read when More opens, the only time it is shown there.
  const libraryDetail = menuOpen ? libraryMessages.detail(library, Date.now(), (status) => zoteroLibraryNames(status, groupName)) : null;
  return <PickerFooter library={library} groupName={groupName} emptyAction={emptyAction}
    status={<PickerLibraryStatus library={library} groupName={groupName} notice={notices} isOpen={libraryMenuOpen}
      onOpenChange={(open) => { if (open) { cancelCopy(); setMenuOpen(false); } setLibraryMenuOpen(open); }}
      searchRef={input} sourceAction={sourceAction} sourceFailure={sourceOpen.failure !== null}
      statusIcon={source.status_icon ?? null} hideAction={sourceState.libraryChoices}
      rowAction={menuOpen && api.openLibrarySource && sourceState.sourceAvailable ? sourceAction.label : undefined}
      zotero={library?.zotero && chooseZoteroLibraries ? {
        offered: ["user" as const, ...offeredGroups.map((group) => ({ group }))], isChosen, chosenCount: chosenLibraries.length,
        name: (choice) => typeof choice === "object" ? groupName(choice.group) : pickerText.myLibrary, toggle: toggleLibrary, all: searchAllLibraries,
      } : null}
      actions={{ choose: chooseSources, find: chooseFile, reload: () => runFromMenu("reload", reloadLibrary),
        useZotero: () => runFromMenu("zotero", useZotero), openZotero: () => openSource(source.kind !== "zotero") }}
      groupsFailed={failures.zoteroGroups} failure={sourceOpen.failure?.message ?? [...libraryOperations].filter((operation) => operation !== "zoteroGroups" && failures[operation]).map((operation) => FAILURE_MESSAGES[operation]).join(" ")} />}

    hints={hints}
    altHeld={altHeld} actions={{ copy: copyChosen, add: toggleHighlighted, escape: escapeStep }}
    more={
      <PickerMenu isOpen={menuOpen} onOpenChange={(open) => (open ? openMore() : closeMenu(false))}
        buttonRef={moreButton} searchRef={input} libraryDetail={libraryDetail}
        source={api.openLibrarySource && sourceState.sourceAvailable ? sourceAction : undefined}
        lastOutcome={lastOutcome}
        project={{ status: project.status, choose: project.choose, stop: project.stop,
          copyEntry: highlighted?.project?.state === "missing" && highlighted.project.generation === project.status?.generation
            ? () => { project.copyEntry(highlighted); input.current?.focus(); } : null }}
        density={density} pinned={pinned}
        actions={{
          density: changeDensity,
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
    } />;
}
