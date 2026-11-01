import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { PickerApi } from "../../App";
import type { Operation } from "./Notices";
import type { useOperations } from "./useOperations";
import type { usePickerLibrary } from "./usePickerLibrary";
import { usePin } from "./usePin";
import { useWindowResetUndo } from "./useWindowResetUndo";

export function usePickerMenus(api: PickerApi, { perform }: Pick<ReturnType<typeof useOperations>, "perform">,
  { choosingLibrary, welcomeShown }: Pick<ReturnType<typeof usePickerLibrary>, "choosingLibrary" | "welcomeShown">, cancelCopy: () => void) {
  const { setPinned } = api;
  const input = useRef<HTMLInputElement>(null);
  const formatButtonRef = useRef<HTMLButtonElement>(null);
  const moreButton = useRef<HTMLButtonElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [libraryMenuOpen, setLibraryMenuOpen] = useState(false);
  const [formatOpen, setFormatOpen] = useState(false);
  const { pinned, currentPin, togglePin } = usePin({ setPinned, perform });
  const [keysOpen, setKeysOpen] = useState(false);
  const restoreChooserFocus = useRef(false);
  useLayoutEffect(() => {
    if (choosingLibrary || !restoreChooserFocus.current) return;
    restoreChooserFocus.current = false;
    // Welcome hides Search with CSS. Inspect the committed screen, after the
    // chooser leaves, so focus never goes to the header it just replaced.
    const welcome = input.current?.closest("main")?.querySelector(".library-welcome");
    (welcome ? moreButton.current : input.current)?.focus();
  }, [choosingLibrary]);
  // Setup's chosen choice leaves with setup. If nothing else took focus, the
  // keyboard goes to Search; otherwise keys would reach no handler at all.
  const welcomeWasShown = useRef(welcomeShown);
  useLayoutEffect(() => {
    const left = welcomeWasShown.current && !welcomeShown;
    welcomeWasShown.current = welcomeShown;
    if (left && (document.activeElement === null || document.activeElement === document.body)) input.current?.focus();
  }, [welcomeShown]);
  const windowUndo = useWindowResetUndo(api);
  useEffect(() => { if (windowUndo.ticket !== null) { setKeysOpen(false); setMenuOpen(false); setLibraryMenuOpen(false); } }, [windowUndo.ticket]);
  // Where focus goes back to when the keys overlay closes.
  const keysReturn = useRef<HTMLElement | null>(null);
  const openMore = () => {
    cancelCopy();
    setLibraryMenuOpen(false);
    // React Aria records the invoker before mounting its modal focus scope.
    moreButton.current?.focus();
    setMenuOpen(true);
  };
  const closeMenu = (refocus: boolean) => {
    setMenuOpen(false);
    setLibraryMenuOpen(false);
    if (refocus) moreButton.current?.focus();
  };
  const focusPicker = () => (welcomeShown ? moreButton.current : input.current)?.focus();
  const runFromMenu = (operation: Operation, action: () => Promise<unknown>) => {
    cancelCopy();
    closeMenu(false);
    focusPicker();
    void perform(operation, action);
  };
  const openKeys = () => {
    cancelCopy();
    closeMenu(false);
    const focused = document.activeElement;
    // A dismissed menu item returns to a visible control; welcome has no Search.
    keysReturn.current = focused instanceof HTMLElement && !focused.closest("[role='menu']") ? focused
      : welcomeShown ? moreButton.current : input.current;
    setKeysOpen(true);
  };
  const closeKeys = () => {
    setKeysOpen(false);
    const back = keysReturn.current;
    const fallback = welcomeShown ? moreButton.current : input.current;
    (back?.isConnected ? back : fallback)?.focus();
  };
  return { input, formatButtonRef, moreButton, menuOpen, setMenuOpen, libraryMenuOpen, setLibraryMenuOpen, formatOpen, setFormatOpen, pinned, currentPin, togglePin, keysOpen, setKeysOpen, restoreChooserFocus, windowUndo, openMore, closeMenu, focusPicker, runFromMenu, openKeys, closeKeys };
}
