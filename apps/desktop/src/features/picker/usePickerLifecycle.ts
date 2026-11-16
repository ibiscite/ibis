import type * as nativeApi from "../../api";
import { useEffect } from "react";
import { flushSync } from "react-dom";
import type { usePickerLibrary } from "./usePickerLibrary";
import type { useRecentReferences } from "./useRecentReferences";
import type { Operation } from "./Notices";
import { closeOverlays } from "../../ui/layers";
import type { useOperations } from "./useOperations";
import type { usePickerSession } from "./usePickerSession";
import type { usePickerMenus } from "./usePickerMenus";
import type { useDestination } from "./useDestination";
import type { usePickerPreferences } from "./usePickerPreferences";
import { listening } from "../../nativeEvents";

export function usePickerLifecycle({ api, pickerSession, menus, destination, preferences, libraryState, recents, resetPointer, readShortcut, report, setAltHeld }: {
  api: typeof nativeApi;
  pickerSession: Pick<ReturnType<typeof usePickerSession>, "session" | "send">;
  menus: Pick<ReturnType<typeof usePickerMenus>, "input" | "focusHome">;
  destination: ReturnType<typeof useDestination>;
  preferences: ReturnType<typeof usePickerPreferences>;
  libraryState: ReturnType<typeof usePickerLibrary>;
  recents: ReturnType<typeof useRecentReferences>;
  resetPointer: () => void;
  readShortcut: () => void;
  report: ReturnType<typeof useOperations>["report"];
  setAltHeld: (held: boolean) => void;
}) {
  const { session, send } = pickerSession;
  const { input, focusHome } = menus;
  const { pickerSnapshot, pickerRendered, onPickerPreparing, onPickerShown, onPickerHidden, onPreferencesChanged, onProjectChanged, onWritingsChanged } = api;
  // When the picker is shown, put the cursor in the search box with the
  // previous query selected, so typing replaces it.
  // biome-ignore lint/correctness/useExhaustiveDependencies: subscribes once on mount. The handlers only call setters, refs and native calls that never change, so the first render's copies are current.
  useEffect(() => {
    let mounted = true;
    let focusAfterActivation = false;
    let searchAtShow = "";
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
    let presentation = -1;
    // A detour's citation and a change coming back keep the output they were made in.
    const keepsOutput = () => { const { permissionDetour, changing } = session.getSnapshot(); return permissionDetour || changing !== null; };
    const focusSearch = () => {
      readShortcut();
      searchAtShow = input.current?.value ?? "";
      // On X11, showing can precede native activation. WebKit restores the
      // previous control then, so finish Search focus once activation arrives.
      focusAfterActivation = !document.hasFocus();
      selectSearch();
    };
    // Every hide is reported, including the native one when focus moves away;
    // a monotonic clock keeps clock changes from clearing or keeping a search.
    const reporting = (operation: Operation) => ({ failed: () => report(operation), subject: "Picker" });
    // Reserve admission before subscribing: a newer live observation retires
    // only its own part of a delayed bootstrap response.
    const receivers = () => ({
      destination: destination.snapshotReceiver(), library: libraryState.snapshotReceiver(),
      preferences: preferences.snapshotReceiver(), recents: recents.snapshotReceiver(),
    });
    const bootstrap = receivers();
    const accept = (snapshot: nativeApi.PickerSnapshot | null, receive = receivers()) => {
      if (!mounted || !snapshot || snapshot.presentation <= presentation) return;
      presentation = snapshot.presentation;
      const preserve = keepsOutput();
      flushSync(() => {
        resetPointer();
        send({ type: "preparing", now: performance.now(), selection: snapshot.selection });
        receive.destination(snapshot.destination, preserve);
        receive.library(snapshot.library);
        receive.preferences("Ok" in snapshot.preferences ? snapshot.preferences.Ok : null);
        receive.recents(snapshot.recents, snapshot.library.revision, "Ok" in snapshot.destination ? snapshot.destination.Ok : null);
        send({ type: "shown", now: performance.now(), presentation: snapshot.presentation });
      });
      void pickerRendered(snapshot.presentation).catch(() => report("pickerEvents"));
    };
    const stops = [
      listening(onPickerHidden, () => {
        focusAfterActivation = false;
        resetPointer();
        send({ type: "hidden", now: performance.now() });
        closeOverlays();
        setAltHeld(false);
      }, reporting("pickerEvents")),
      listening(onPickerPreparing, accept, { ...reporting("pickerEvents"), ready: () => {
        void pickerSnapshot().then((snapshot) => accept(snapshot, bootstrap), () => report("pickerEvents"));
      } }),
      listening(onPickerShown, focusSearch, reporting("pickerEvents")),
    ];
    stops.push(
      // Choices made in Settings apply to the picker at once.
      listening(onPreferencesChanged, (change) => { preferences.preferencesChanged(change?.revision, change?.fields, () => mounted); destination.read(false); }),
      listening(onProjectChanged, (version) => destination.writingChanged(version), reporting("project")),
      listening(onWritingsChanged, () => destination.read(false), reporting("project")),
    );
    return () => {
      mounted = false;
      window.removeEventListener("focus", activated);
      send({ type: "retire" });
      for (const stop of stops) stop();
    };
  }, [resetPointer]);
}
