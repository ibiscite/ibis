import { useCallback, useEffect } from "react";
import type { PickerApi } from "../../App";
import { keysFor } from "../../keys";
import { isolate, say } from "../../i18n";
import { isMac } from "../../platform";
import { SourceIcon, type SourceActionProps } from "../../ui/SourceAction";
import type { usePickerLibrary } from "./usePickerLibrary";
import type { usePickerMenus } from "./usePickerMenus";
import type { usePickerSession } from "./usePickerSession";
import type { useOperations } from "./useOperations";

/** "Find thesis.bib…": the action when the library file has moved. */
export const findSourceLabel = (name: string | null | undefined) => name ? say("picker-find-source", { name: isolate(name) }) : say("picker-find-source-unnamed");
import type { useLibrarySearch } from "./useLibrarySearch";
import { closeOverlays } from "../../ui/layers";
import { useSourceOpen } from "./useSourceOpen";
import { zoteroWaits } from "./PickerLibraryStatus";

export function usePickerSource({ api, pickerSession, libraryState, menus, results, altHeld, perform, report }: {
  api: PickerApi;
  pickerSession: ReturnType<typeof usePickerSession>;
  libraryState: ReturnType<typeof usePickerLibrary>;
  menus: ReturnType<typeof usePickerMenus>;
  /** The shown results: the source follows the highlighted row on screen. */
  results: ReturnType<typeof useLibrarySearch>["results"];
  altHeld: boolean;
  perform: ReturnType<typeof useOperations>["perform"];
  report: ReturnType<typeof useOperations>["report"];
}) {
  const { session, state, cancelCopy } = pickerSession;
  const { library, libraryChoices, chooserRequest, setChoosingLibrary, refreshSnapshot } = libraryState;
  const { input, leaveOverlays, runFromMenu } = menus;
  const { active, selected } = state;
  const { chooseLibrary, onLibraryFileDropped, onLibraryFileRefused } = api;
  const sourceItem = !libraryChoices ? results[active]?.id ?? (selected.size === 1 ? [...selected.values()][0]?.id : null) ?? null : null;
  const sourceOpen = useSourceOpen(api, library, state.visibility, sourceItem, false, refreshSnapshot, () => session.getSnapshot().presentation);
  const chooseSources = () => {
    cancelCopy(); closeOverlays();
    chooserRequest.current = {};
    setChoosingLibrary(true);
  };
  const dismissChooser = useCallback(() => {
    chooserRequest.current = null;
    setChoosingLibrary(false);
  }, [chooserRequest, setChoosingLibrary]);
  useEffect(() => {
    let active = true;
    const subscription = onLibraryFileDropped?.(() => {
      if (active) { report("dropRefused", false); dismissChooser(); }
    }).catch(() => () => undefined);
    const refusal = onLibraryFileRefused?.(() => {
      if (active) report("dropRefused");
    }).catch(() => () => undefined);
    return () => {
      active = false;
      void subscription?.then((stop) => stop()).catch(() => undefined);
      void refusal?.then((stop) => stop()).catch(() => undefined);
    };
  }, [onLibraryFileDropped, onLibraryFileRefused, dismissChooser, report]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a refusal belongs to the opening it happened in.
  useEffect(() => { report("dropRefused", false); }, [state.visibility, report]);
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
  const missingSource = library?.problem === "missing" && !library.zotero && (!library.has_snapshot || library.stale ||
    library.references === 0 && library.source_is_requested);
  // While opening Zotero is the fix, ⌘O opens Zotero: the footer's fix and More
  // then name the same target, and the words always name what ⌘O does.
  const zoteroFix = Boolean(library?.zotero && (library.error || library.stale) && zoteroWaits(library.problem));
  const openSource = (appOnly = false, reveal = altHeld) => {
    leaveOverlays();
    appOnly ||= zoteroFix && sourceOpen.source.kind === "zotero";
    if (missingSource && !appOnly) { chooseFile(); return; }
    if (sourceOpen.failure && !sourceOpen.failure.changed && !appOnly) {
      if (sourceOpen.failure.kind === "file" && library?.name) chooseFile();
      else chooseSources();
    } else sourceOpen.run(reveal, appOnly);
  };
  const source = sourceOpen.source;
  const sourceLabel = missingSource ? findSourceLabel(library?.name) : sourceOpen.failure && !sourceOpen.failure.changed ? sourceOpen.failure.kind === "file" && library?.name ? findSourceLabel(source.name) : say("picker-choose-library-menu")
    : source.kind === "zotero" ? sourceItem && !zoteroFix ? say("picker-show-in-zotero") : say("picker-open-zotero")
    : altHeld ? say("picker-show-source-folder", { platform: isMac ? "macos" : "other" }) : (source.name ? say("picker-open-source", { name: isolate(source.name) }) : say("picker-open-library"));
  const sourceAction: SourceActionProps = { label: sourceLabel, keys: keysFor(altHeld && source.kind === "file" ? "revealSource" : "source"), icon: source.icon,
    file: source.kind === "file", busy: sourceOpen.busy, open: () => openSource() };
  const sourceInChoices = !(sourceOpen.failure && !sourceOpen.failure.changed) && !libraryState.choosingLibrary && Boolean(library?.name) && Boolean(libraryChoices) && (!library?.zotero || zoteroWaits(library.problem));
  const sourceAvailable = source.kind !== "zotero" || sourceItem !== null || Boolean(library?.zotero && zoteroWaits(library.problem));
  const sourceChoice = { ...sourceAction, icon: <SourceIcon icon={source.icon} file={source.kind === "file"} /> };
  /** A reference's menu opens that reference, and recovers its file like More does. */
  const openReference = (id: unknown) => {
    if (missingSource || sourceOpen.failure) openSource();
    else sourceOpen.run(false, false, id);
  };
  return { sourceAvailable, libraryChoices, sourceInChoices, sourceOpen, source, sourceAction, sourceChoice, chooseSources, dismissChooser, performSourceChoice, chooseFile, openSource, openReference };
}
