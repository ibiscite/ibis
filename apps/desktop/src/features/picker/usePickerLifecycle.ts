import type * as nativeApi from "../../api";
import { useEffect } from "react";
import type { Operation } from "./Notices";
import { closeOverlays } from "../../ui/layers";
import type { useOperations } from "./useOperations";
import type { usePickerSession } from "./usePickerSession";
import type { usePickerMenus } from "./usePickerMenus";
import type { useDestination } from "./useDestination";
import type { usePickerPreferences } from "./usePickerPreferences";
import { listening } from "../../nativeEvents";

export function usePickerLifecycle({ api, pickerSession, menus, destination, preferences, resetPointer, readShortcut, report, setAltHeld }: {
  api: typeof nativeApi;
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
  const { pickerVisible, onPickerPreparing, onPickerShown, onPickerHidden, onPreferencesChanged, onProjectChanged, onWritingsChanged } = api;
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
      // Reopening also sees file edits whose watch event has not arrived.
      // The settings owner reads after prior saves; focus never waits for it,
      // and a reply from an older presentation cannot replace this one.
      const visibility = session.getSnapshot().visibility;
      preferences.loadPreferences(() => mounted && session.getSnapshot().visibility === visibility);
      // On X11, showing can precede native activation. WebKit restores the
      // previous control then, so finish Search focus once activation arrives.
      focusAfterActivation = !document.hasFocus();
      selectSearch();
    };
    // Every hide is reported, including the native one when focus moves away;
    // a monotonic clock keeps clock changes from clearing or keeping a search.
    const reporting = (operation: Operation) => ({ failed: () => report(operation), subject: "Picker" });
    // Visibility is read once hiding and showing are both heard (or refused), so neither is missed.
    const heard = () => {
      let settle = () => {};
      const settled = new Promise<void>((done) => { settle = done; });
      return { settled, options: { ready: settle, failed: () => { report("pickerEvents"); settle(); }, subject: "Picker" } };
    };
    const hidden = heard(), shown = heard();
    void Promise.all([hidden.settled, shown.settled]).then(async () => {
      send({ type: "initialize" });
      const events = session.getSnapshot().visibilityEvents;
      const visible = await pickerVisible();
      // The session keeps a newer event over this read; a failed read changes nothing.
      if (mounted) send({ type: "visibility-read", visible, events });
    }).catch(() => undefined);
    const stops = [
      listening(onPickerHidden, () => {
        focusAfterActivation = false;
        resetPointer();
        send({ type: "hidden", now: performance.now() });
        closeOverlays();
        setAltHeld(false);
      }, hidden.options),
      // The window appears after this: settle what its first frame shows.
      listening(onPickerPreparing, (selection) => {
        prepared = true;
        send({ type: "preparing", now: performance.now(), selection });
        destination.read(true, keepsOutput());
      }, reporting("pickerEvents")),
      listening(onPickerShown, focusSearch, shown.options),
    ];
    destination.read();
    preferences.loadPreferences(() => mounted);
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
