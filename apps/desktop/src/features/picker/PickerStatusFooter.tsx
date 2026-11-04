import type { ReactNode } from "react";
import { libraryMessages, operationFailures as FAILURE_MESSAGES, pickerText } from "../../messages";
import { zoteroLibraryNames } from "../../LibraryStatusLine";
import type { PickerApi } from "../../App";
import type { LibraryStatus } from "../../api";
import { PickerFooter } from "./PickerFooter";
import { PickerLibraryStatus } from "./PickerLibraryStatus";
import { PickerMenu } from "./PickerMenu";
import { libraryOperations } from "./Notices";
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

export function PickerStatusFooter({
  api, library, scope, sourceState, menus, actions, run, copyChosen, destination, recents, preferences,
  operations, altHeld, lastOutcome, cancelCopy, emptyAction, notices,
}: {
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
  cancelCopy: () => void;
  emptyAction?: boolean;
  notices?: ReactNode;
}) {
  const { useZoteroLibraries: chooseZoteroLibraries, reloadLibrary, useZotero, openSettings } = api;
  const { groupName, offeredGroups, isChosen, chosenLibraries, toggleLibrary, searchAllLibraries } = scope;
  const { libraryMenuOpen, openLibrary, closeLibrary, input, menuOpen, openMore, closeMore, moreButton, pinned, focusHome, runFromMenu } = menus;
  const { sourceAction, sourceOpen, source, chooseSources, chooseFile, openSource } = sourceState;
  const { density, changeDensity } = preferences;
  const { failures, report } = operations;
  const { toggleHighlighted, hints } = actions;
  // Read when More opens, the only time it is shown there.
  const libraryDetail = menuOpen ? libraryMessages.detail(library, Date.now(), (status) => zoteroLibraryNames(status, groupName)) : null;
  return <PickerFooter library={library} groupName={groupName} emptyAction={emptyAction}
    status={<PickerLibraryStatus library={library} groupName={groupName} notice={notices} isOpen={libraryMenuOpen}
      onOpenChange={(open) => (open ? openLibrary() : closeLibrary())}
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
    altHeld={altHeld} actions={{ copy: copyChosen, add: toggleHighlighted, escape: () => closeNearestLayer() }}
    more={
      <PickerMenu isOpen={menuOpen} onOpenChange={(open) => (open ? openMore() : closeMore())}
        buttonRef={moreButton} searchRef={input} libraryDetail={libraryDetail}
        source={api.openLibrarySource && sourceState.sourceAvailable ? sourceAction : undefined}
        lastOutcome={lastOutcome}
        density={density} pinned={pinned}
        actions={{
          density: changeDensity,
          pin: () => run("pin"),
          keys: () => run("keys"),
          changeLast: actions.canChangeLast ? () => run("changeLast") : null,
          settings: openSettings ? () => run("settings") : null,
          copyOnlyHere: destination.pasteChoices && destination.app ? { app: destination.app, choose: () => {
            cancelCopy();
            destination.choosePaste("copy-only");
          } } : null,
          forgetRecent: recents.forget ? () => { report("recentHistory", !recents.forget!()); focusHome(); } : null,
          quit: () => run("quit"),
        }} />
    } />;
}
