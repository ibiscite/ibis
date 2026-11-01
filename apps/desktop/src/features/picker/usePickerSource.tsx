import type { PickerApi } from "../../App";
import { keysFor } from "../../keys";
import { pickerText } from "../../messages";
import { isMac } from "../../platform";
import { SourceIcon, type SourceActionProps } from "../../ui/SourceAction";
import type { usePickerLibrary } from "./usePickerLibrary";
import type { usePickerMenus } from "./usePickerMenus";
import type { usePickerSession } from "./usePickerSession";
import type { useOperations } from "./useOperations";
import type { useLibrarySearch } from "./useLibrarySearch";
import { useSourceOpen } from "./useSourceOpen";

export function usePickerSource({ api, pickerSession, libraryState, menus, current, altHeld, perform }: {
  api: PickerApi;
  pickerSession: ReturnType<typeof usePickerSession>;
  libraryState: ReturnType<typeof usePickerLibrary>;
  menus: ReturnType<typeof usePickerMenus>;
  current: ReturnType<typeof useLibrarySearch>["current"];
  altHeld: boolean;
  perform: ReturnType<typeof useOperations>["perform"];
}) {
  const { session, state, cancelCopy } = pickerSession;
  const { library, libraryChoices, chooserRequest, setChoosingLibrary, refreshSnapshot } = libraryState;
  const { input, closeMenu, setKeysOpen, restoreChooserFocus, runFromMenu, focusPicker } = menus;
  const { active, selected } = state;
  const { chooseLibrary } = api;
  const sourceItem = !libraryChoices ? current[active]?.id ?? (selected.size === 1 ? [...selected.values()][0]?.id : null) ?? null : null;
  const sourceOpen = useSourceOpen(api, library, state.visibility, sourceItem, false, refreshSnapshot);
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
      if (sourceOpen.failure.kind === "file" && library?.name) chooseFile();
      else chooseSources();
    } else sourceOpen.run(reveal, appOnly);
  };
  const source = sourceOpen.source;
  const sourceLabel = missingSelected ? pickerText.findSourceFile(library?.name ?? null) : sourceOpen.failure && !sourceOpen.failure.changed ? sourceOpen.failure.kind === "file" && library?.name ? pickerText.findSourceFile(source.name) : pickerText.chooseLibraryMenu
    : source.kind === "zotero" ? sourceItem ? pickerText.showInZotero : pickerText.openZotero
    : altHeld ? pickerText.showSourceFolder(isMac) : pickerText.openSource(source.name);
  const sourceAction: SourceActionProps = { label: sourceLabel, keys: keysFor(altHeld && source.kind === "file" ? "revealSource" : "source"), icon: source.icon,
    file: source.kind === "file", busy: sourceOpen.busy, open: () => openSource() };
  const sourceInChoices = !(sourceOpen.failure && !sourceOpen.failure.changed) && !libraryState.choosingLibrary && Boolean(library?.name) && Boolean(libraryChoices) && (!library?.zotero || library.problem === "zotero_closed");
  const sourceAvailable = source.kind !== "zotero" || sourceItem !== null || Boolean(library?.zotero && library.problem === "zotero_closed");
  const sourceChoice = { ...sourceAction, icon: <SourceIcon icon={source.icon} file={source.kind === "file"} /> };
  /** A reference's menu opens that reference, and recovers its file like More does. */
  const openReference = (id: unknown) => {
    if (missingSelected || sourceOpen.failure) openSource();
    else sourceOpen.run(false, false, id);
  };
  return { sourceAvailable, libraryChoices, sourceInChoices, sourceOpen, source, sourceAction, sourceChoice, chooseSources, dismissChooser, performSourceChoice, chooseFile, openSource, openReference };
}
