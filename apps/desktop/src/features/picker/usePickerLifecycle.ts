import { useEffect } from "react";
import type { PickerApi } from "../../App";
import { operationFailures as FAILURE_MESSAGES } from "../../messages";
import type { Operation } from "./Notices";
import type { useOperations } from "./useOperations";
import type { usePickerSession } from "./usePickerSession";
import type { usePickerMenus } from "./usePickerMenus";
import type { useDestination } from "./useDestination";
import type { usePickerPreferences } from "./usePickerPreferences";

export function usePickerLifecycle({ api, pickerSession, menus, destination, preferences, resetPointer, readShortcut, report, setAltHeld }: {
  api: PickerApi;
  pickerSession: Pick<ReturnType<typeof usePickerSession>, "session" | "send">;
  menus: Pick<ReturnType<typeof usePickerMenus>, "input" | "setKeysOpen" | "setLibraryMenuOpen" | "setMenuOpen">;
  destination: ReturnType<typeof useDestination>;
  preferences: ReturnType<typeof usePickerPreferences>;
  resetPointer: () => void;
  readShortcut: () => void;
  report: ReturnType<typeof useOperations>["report"];
  setAltHeld: (held: boolean) => void;
}) {
  const { session, send } = pickerSession;
  const { input, setKeysOpen, setLibraryMenuOpen, setMenuOpen } = menus;
  const { pickerVisible, onPickerPreparing, onPickerShown, onPickerHidden, onPreferencesChanged } = api;
  // When the picker is shown, put the cursor in the search box with the
  // previous query selected, so typing replaces it.
  // biome-ignore lint/correctness/useExhaustiveDependencies: subscribes once on mount. The handlers only call setters, refs and native calls that never change, so the first render's copies are current.
  useEffect(() => {
    let mounted = true;
    let visibilityEvents = 0;
    let focusAfterActivation = false;
    let searchAtShow = "";
    const subscriptionFailure = (operation: Operation) => {
      if (mounted) report(operation);
      else console.error(FAILURE_MESSAGES[operation]);
      return () => undefined;
    };
    const selectSearch = () => {
      input.current?.focus();
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
    const focusSearch = (presentation: number) => {
      searchAtShow = input.current?.value ?? "";
      resetPointer();
      const previousQuery = session.getSnapshot().query;
      send({ type: "shown", now: performance.now(), presentation });
      if (session.getSnapshot().query !== previousQuery) searchAtShow = session.getSnapshot().query;
      if (!prepared) destination.read(true, session.getSnapshot().permissionDetour);
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
      visibilityEvents += 1;
      focusAfterActivation = false;
      // A hidden native window may not throttle the web view: pause looping animations.
      document.documentElement.dataset.pickerHidden = "";
      resetPointer();
      send({ type: "hidden", now: performance.now() });
      setKeysOpen(false);
      setLibraryMenuOpen(false);
      setMenuOpen(false);
      setAltHeld(false);
    })
      .catch(() => subscriptionFailure("pickerEvents"));
    // The window appears after this: settle what its first frame shows.
    const unlistenPreparing = onPickerPreparing?.(() => {
      if (!mounted) return;
      prepared = true;
      send({ type: "preparing", now: performance.now() });
      destination.read(true, session.getSnapshot().permissionDetour);
    }).catch(() => subscriptionFailure("pickerEvents"));
    const unlisten = onPickerShown((presentation) => {
      if (!mounted) return;
      visibilityEvents += 1;
      delete document.documentElement.dataset.pickerHidden;
      focusSearch(presentation);
    }).catch(() => subscriptionFailure("pickerEvents"));
    void Promise.all([unlistenHidden, unlisten]).then(async () => {
      if (!mounted) return;
      send({ type: "initialize" });
      const events = visibilityEvents;
      const visible = await pickerVisible();
      // A newer event wins over this read; failed reads leave the flag alone.
      if (!mounted || events !== visibilityEvents) return;
      if (visible) delete document.documentElement.dataset.pickerHidden;
      else document.documentElement.dataset.pickerHidden = "";
    }).catch(() => undefined);
    destination.read();
    preferences.loadPreferences(() => mounted);
    // Choices made in Settings apply to the picker at once.
    const unlistenPreferences = onPreferencesChanged?.(() => { preferences.loadPreferences(() => mounted); destination.read(false); })
      .catch(() => undefined);
    return () => {
      mounted = false;
      window.removeEventListener("focus", activated);
      send({ type: "retire" });
      void unlistenHidden.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
      void unlisten.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
      void unlistenPreparing?.then((stop) => stop?.());
      void unlistenPreferences?.then((stop) => stop?.());
    };
  }, [resetPointer]);
}
