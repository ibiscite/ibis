import { useEffect } from "react";
import type { PickerApi } from "../../App";
import type { Operation } from "./Notices";
import { closeOverlays } from "../../ui/layers";
import type { useOperations } from "./useOperations";
import type { usePickerSession } from "./usePickerSession";
import type { usePickerMenus } from "./usePickerMenus";
import type { useDestination } from "./useDestination";
import type { usePickerPreferences } from "./usePickerPreferences";
import { OPERATION_FAILURES } from "./Notices";
import { say } from "../../i18n";

export function usePickerLifecycle({ api, pickerSession, menus, destination, preferences, resetPointer, readShortcut, report, setAltHeld }: {
  api: PickerApi;
  pickerSession: Pick<ReturnType<typeof usePickerSession>, "session" | "send">;
  menus: Pick<ReturnType<typeof usePickerMenus>, "input" | "focusHome">;
  destination: ReturnType<typeof useDestination>;
  preferences: ReturnType<typeof usePickerPreferences>;
  resetPointer: () => void;
  readShortcut: () => void;
  report: ReturnType<typeof useOperations>["report"];
  setAltHeld: (held: boolean) => void;
}) {
  const { session, send } = pickerSession;
  const { input, focusHome } = menus;
  const { pickerVisible, onPickerPreparing, onPickerShown, onPickerHidden, onPreferencesChanged, onProjectChanged, onWritingProjectsChanged } = api;
  // When the picker is shown, put the cursor in the search box with the
  // previous query selected, so typing replaces it.
  // biome-ignore lint/correctness/useExhaustiveDependencies: subscribes once on mount. The handlers only call setters, refs and native calls that never change, so the first render's copies are current.
  useEffect(() => {
    let mounted = true;
    let focusAfterActivation = false;
    let searchAtShow = "";
    const subscriptionFailure = (operation: Operation) => {
      if (mounted) report(operation);
      else console.error(say(OPERATION_FAILURES[operation]));
      return () => undefined;
    };
    const selectSearch = () => {
      focusHome();
      input.current?.select();
    };
    const activated = (event: FocusEvent) => {
      if (event.target !== event.currentTarget || !focusAfterActivation) return;
      focusAfterActivation = false;
      const search = input.current;
      if (!search) return;
      search.focus();
      if (search.value === searchAtShow) search.select();
      else search.setSelectionRange(search.value.length, search.value.length);
    };
    window.addEventListener("focus", activated);
    let prepared = false;
    // A detour's citation and a change coming back keep the output they were made in.
    const keepsOutput = () => { const { permissionDetour, changing } = session.getSnapshot(); return permissionDetour || changing !== null; };
    const focusSearch = (presentation: number) => {
      searchAtShow = input.current?.value ?? "";
      resetPointer();
      const previousQuery = session.getSnapshot().query;
      send({ type: "shown", now: performance.now(), presentation });
      if (session.getSnapshot().query !== previousQuery) searchAtShow = session.getSnapshot().query;
      if (!prepared) destination.read(true, keepsOutput());
      prepared = false;
      readShortcut();
      // On X11, showing can precede native activation. WebKit restores the
      // previous control then, so finish Search focus once activation arrives.
      focusAfterActivation = !document.hasFocus();
      selectSearch();
    };
    // Every hide is reported, including the native one when focus moves away;
    // a monotonic clock keeps clock changes from clearing or keeping a search.
    const unlistenHidden = onPickerHidden(() => {
      if (!mounted) return;
      focusAfterActivation = false;
      resetPointer();
      send({ type: "hidden", now: performance.now() });
      closeOverlays();
      setAltHeld(false);
    })
      .catch(() => subscriptionFailure("pickerEvents"));
    // The window appears after this: settle what its first frame shows.
    const unlistenPreparing = onPickerPreparing?.((selection) => {
      if (!mounted) return;
      prepared = true;
      send({ type: "preparing", now: performance.now(), selection });
      destination.read(true, keepsOutput());
    }).catch(() => subscriptionFailure("pickerEvents"));
    const unlisten = onPickerShown((presentation) => {
      if (!mounted) return;
      focusSearch(presentation);
    }).catch(() => subscriptionFailure("pickerEvents"));
    void Promise.all([unlistenHidden, unlisten]).then(async () => {
      if (!mounted) return;
      send({ type: "initialize" });
      const events = session.getSnapshot().visibilityEvents;
      const visible = await pickerVisible();
      // The session keeps a newer event over this read; a failed read changes nothing.
      if (mounted) send({ type: "visibility-read", visible, events });
    }).catch(() => undefined);
    destination.read();
    preferences.loadPreferences(() => mounted);
    // Choices made in Settings apply to the picker at once.
    const unlistenPreferences = onPreferencesChanged?.((change) => { preferences.preferencesChanged(change?.revision, change?.fields, () => mounted); destination.read(false); })
      .catch(() => undefined);
    const unlistenProject = onProjectChanged?.((version) => {
      if (!mounted) return;
      destination.projectChanged(version);
    }).catch(() => subscriptionFailure("project"));
    const unlistenWritingProjects = onWritingProjectsChanged?.(() => {
      if (mounted) destination.read(false);
    }).catch(() => subscriptionFailure("project"));
    return () => {
      mounted = false;
      window.removeEventListener("focus", activated);
      send({ type: "retire" });
      void unlistenHidden.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
      void unlisten.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
      void unlistenPreparing?.then((stop) => stop?.());
      void unlistenPreferences?.then((stop) => stop?.());
      void unlistenProject?.then((stop) => stop?.());
      void unlistenWritingProjects?.then((stop) => stop?.());
    };
  }, [resetPointer]);
}
