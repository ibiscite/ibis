// Which Zotero libraries a search covers, and the group names shown for them.
import { useEffect, useState } from "react";
import type * as nativeApi from "../../api";
import type { LibraryStatus, ZoteroGroup, ZoteroLibrary } from "../../api";
import { pickerText } from "../../messages";
import { sameLibrary } from "./choice";
import type { useOperations } from "./useOperations";

export function useZoteroScope({ library, libraryVersion, menuOpen, nothingMatched, zoteroGroups, chooseZoteroLibraries, report, perform, focusSearch }: {
  library: LibraryStatus | null | undefined;
  libraryVersion: number;
  menuOpen: boolean;
  nothingMatched: boolean;
  zoteroGroups?: typeof nativeApi.zoteroGroups;
  chooseZoteroLibraries?: typeof nativeApi.useZoteroLibraries;
  report: ReturnType<typeof useOperations>["report"];
  perform: ReturnType<typeof useOperations>["perform"];
  focusSearch: () => void;
}) {
  // Zotero libraries: the ones chosen, and the groups Zotero offers, listed
  // when the menu opens or when results come from several libraries.
  const [groups, setGroups] = useState<ZoteroGroup[] | null>(null);
  // A choice sent but not yet reported back, so a second toggle builds on it.
  const [pendingLibraries, setPendingLibraries] = useState<ZoteroLibrary[] | null>(null);
  const reportedLibraries = library?.zotero_libraries ?? [];
  const sameChoice = (a: ZoteroLibrary[], b: ZoteroLibrary[]) =>
    a.length === b.length && a.every((choice, i) => sameLibrary(choice, b[i]!));
  // biome-ignore lint/correctness/useExhaustiveDependencies: library is the trigger: a new status report settles the pending choice.
  useEffect(() => {
    if (pendingLibraries && sameChoice(pendingLibraries, reportedLibraries)) setPendingLibraries(null);
  }, [library]);
  const chosenLibraries = pendingLibraries ?? reportedLibraries;
  const severalLibraries = chosenLibraries.length > 1;
  const wantGroups = Boolean(zoteroGroups && library?.zotero && (menuOpen || severalLibraries || nothingMatched));
  // biome-ignore lint/correctness/useExhaustiveDependencies: libraryVersion is a trigger: a library change refreshes group names.
  useEffect(() => {
    // Group names can refresh in the background, but a failure belongs only
    // to this menu opening. Closing it also invalidates its pending request.
    report("zoteroGroups", false);
    if (!wantGroups || !zoteroGroups) return;
    let current = true;
    zoteroGroups()
      .then((list) => { if (current) { setGroups(list); report("zoteroGroups", false); } })
      .catch(() => { if (current && menuOpen) report("zoteroGroups"); });
    return () => { current = false; };
  }, [wantGroups, menuOpen, libraryVersion]);
  const isChosen = (choice: ZoteroLibrary) => chosenLibraries.some((chosen) => sameLibrary(chosen, choice));
  const toggleLibrary = (choice: ZoteroLibrary) => {
    if (!chooseZoteroLibraries) return;
    const next = isChosen(choice)
      ? chosenLibraries.filter((chosen) => !sameLibrary(chosen, choice))
      : [...chosenLibraries, choice];
    if (next.length === 0) return;
    setPendingLibraries(next);
    void perform("zotero", () => chooseZoteroLibraries(next).catch((error: unknown) => {
      setPendingLibraries(null);
      throw error;
    }));
  };
  // Every Zotero library on offer, once the group list is known.
  const allLibraries = (): ZoteroLibrary[] => ["user", ...offeredGroups.map((group) => ({ group }))];
  const searchAllLibraries = () => {
    if (!chooseZoteroLibraries) return;
    const all = allLibraries();
    setPendingLibraries(all);
    focusSearch();
    void perform("zotero", () => chooseZoteroLibraries(all).catch((error: unknown) => {
      setPendingLibraries(null);
      throw error;
    }));
  };
  const groupName = (id: number) => groups?.find((group) => group.id === id)?.name ?? pickerText.groupLibrary(id);
  // Every group Zotero lists, plus chosen groups it no longer lists, so they can be turned off.
  const offeredGroups = [...(groups?.map((group) => group.id) ?? []),
    ...chosenLibraries.flatMap((chosen) => (typeof chosen === "object" ? [chosen.group] : []))]
    .filter((id, i, all) => all.indexOf(id) === i);
  const libraryOf = (source: string) =>
    source === "zotero:user" ? pickerText.myLibrary
      : source.startsWith("zotero:group:") ? groupName(Number(source.slice("zotero:group:".length))) : null;
  const narrowed = Boolean(library?.zotero && chooseZoteroLibraries && groups) && allLibraries().some((choice) => !isChosen(choice));
  return { chosenLibraries, severalLibraries, isChosen, toggleLibrary, searchAllLibraries, groupName, offeredGroups, libraryOf, narrowed };
}
