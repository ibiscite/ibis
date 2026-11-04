import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { PickerApi } from "../../App";
import { closeOverlays, focusLost } from "../../ui/layers";
import type { Operation } from "./Notices";
import type { useOperations } from "./useOperations";
import type { usePickerLibrary } from "./usePickerLibrary";
import { usePin } from "./usePin";
import { useWindowResetUndo } from "./useWindowResetUndo";

/** The picker's own overlay: at most one is open, whichever way it was reached. */
type Overlay = "more" | "library" | "format" | "keys" | "session";

export function usePickerMenus(api: PickerApi, { perform }: Pick<ReturnType<typeof useOperations>, "perform">,
  { welcomeShown }: Pick<ReturnType<typeof usePickerLibrary>, "welcomeShown">, cancelCopy: () => void) {
  const { setPinned } = api;
  const input = useRef<HTMLInputElement>(null);
  const formatButtonRef = useRef<HTMLButtonElement>(null);
  const moreButton = useRef<HTMLButtonElement>(null);
  const [overlay, setOverlay] = useState<Overlay | null>(null);
  const closed = (name: Overlay) => setOverlay((current) => current === name ? null : current);
  const { pinned, currentPin, togglePin } = usePin({ setPinned, perform });
  // Where the keyboard rests. Welcome hides Search; ask the committed screen,
  // because that state arrives a render after the chooser leaves.
  const focusHome = useCallback(() => {
    (input.current?.closest("main")?.querySelector(".library-welcome") ? moreButton.current : input.current)?.focus({ preventScroll: true });
  }, []);
  // Setup's chosen choice leaves with setup. If nothing else took focus, the
  // keyboard goes home.
  const welcomeWasShown = useRef(welcomeShown);
  useLayoutEffect(() => {
    const left = welcomeWasShown.current && !welcomeShown;
    welcomeWasShown.current = welcomeShown;
    if (left && focusLost()) focusHome();
  }, [welcomeShown, focusHome]);
  const windowUndo = useWindowResetUndo(api);
  // A commit cannot flush the closes it causes, so they follow it.
  useEffect(() => { if (windowUndo.ticket !== null) queueMicrotask(closeOverlays); }, [windowUndo.ticket]);
  // Where focus goes back to when a sheet closes.
  const sheetReturn = useRef<HTMLElement | null>(null);
  const openMore = () => {
    cancelCopy();
    closeOverlays();
    // React Aria records the invoker before mounting its modal focus scope.
    moreButton.current?.focus();
    setOverlay("more");
  };
  const openLibrary = () => {
    cancelCopy();
    closeOverlays();
    setOverlay("library");
  };
  const openFormat = () => {
    closeOverlays();
    formatButtonRef.current?.click();
  };
  /** A command that acts on the picker or leaves it: nothing stays in its way. */
  const leaveOverlays = () => {
    cancelCopy();
    closeOverlays();
    focusHome();
  };
  const runFromMenu = (operation: Operation, action: () => Promise<unknown>) => {
    leaveOverlays();
    void perform(operation, action);
  };
  const openSheet = (sheet: "keys" | "session") => {
    cancelCopy();
    closeOverlays();
    const focused = document.activeElement;
    sheetReturn.current = focused instanceof HTMLElement && !focusLost() ? focused : null;
    setOverlay(sheet);
  };
  const closeSheet = (sheet: "keys" | "session") => {
    closed(sheet);
    const back = sheetReturn.current;
    if (back?.isConnected && !back.closest("[hidden]")) back.focus();
    else focusHome();
  };
  return { input, formatButtonRef, moreButton, overlay, menuOpen: overlay === "more", libraryMenuOpen: overlay === "library", formatOpen: overlay === "format", keysOpen: overlay === "keys", sessionOpen: overlay === "session",
    setFormatOpen: (open: boolean) => { if (open) setOverlay("format"); else closed("format"); },
    pinned, currentPin, togglePin, windowUndo, openMore, closeMore: () => closed("more"), openLibrary, closeLibrary: () => closed("library"), openFormat, focusHome, leaveOverlays, runFromMenu,
    openKeys: () => openSheet("keys"), closeKeys: () => closeSheet("keys"),
    openSession: () => openSheet("session"), closeSession: () => closeSheet("session") };
}
