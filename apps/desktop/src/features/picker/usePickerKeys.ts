import { useRef } from "react";
import type { PickerApi } from "../../App";
import { currentPlatform } from "../../platform";
import { focusChoices } from "../../ui/ChoiceList";
import { closeNearestLayer, closeOverlays, composingKey, consumeKey, focusLost, overlayOpen, useKeyOwner } from "../../ui/layers";
import { commandFor, type PickerCommand } from "./shortcuts";
import type { usePickerSession } from "./usePickerSession";
import type { usePickerMenus } from "./usePickerMenus";
import type { usePickerSource } from "./usePickerSource";
import type { useLibrarySearch } from "./useLibrarySearch";
import type { useCitationRows } from "./useCitationRows";
import type { usePickerActions } from "./usePickerActions";
import type { usePointerSelection } from "../../pointerSelection";

/** The picker's one key owner and command path. Every key arrives here first,
 * wherever focus is; a key and its menu item run the same command. */
export function usePickerKeys({
  api, pickerSession, menus, source, results, showCitations, copyChosen, actions, pointer, followKeyboard,
  quitAfterSaving, onEmptyEnter,
}: {
  api: PickerApi;
  pickerSession: Pick<ReturnType<typeof usePickerSession>, "session" | "cancelCopy" | "setActive">;
  menus: ReturnType<typeof usePickerMenus>;
  source: Pick<ReturnType<typeof usePickerSource>, "chooseSources" | "openSource">;
  results: ReturnType<typeof useLibrarySearch>["results"];
  showCitations: boolean;
  copyChosen: ReturnType<typeof useCitationRows>["copyChosen"];
  actions: Pick<ReturnType<typeof usePickerActions>, "toggleHighlighted">;
  pointer: ReturnType<typeof usePointerSelection>;
  followKeyboard: { current: boolean };
  quitAfterSaving: () => Promise<void>;
  onEmptyEnter?: () => void;
}) {
  const { session, cancelCopy, setActive } = pickerSession;
  const { input, moreButton, menuOpen, keysOpen, formatOpen, togglePin, openKeys, openMore, openFormat, focusHome, runFromMenu } = menus;
  const { chooseSources, openSource } = source;
  const { toggleHighlighted } = actions;
  const { openSettings } = api;
  /** Toggles leave layers as they are; a command that opens something first
   * clears them, and pressing an open layer's own key closes it. */
  const run = (command: PickerCommand, event?: KeyboardEvent) => {
    if (command === "pin") togglePin();
    else if (command === "more") {
      if (menuOpen) { closeOverlays(); moreButton.current?.focus(); } else openMore();
    } else if (command === "keys") {
      if (keysOpen) closeNearestLayer(event); else openKeys();
    } else if (command === "format") {
      if (formatOpen) closeNearestLayer(event); else openFormat();
    } else if (command === "chooseSource") chooseSources();
    else if (command === "source" || command === "revealSource") openSource(false, command === "revealSource");
    else if (command === "settings") { if (openSettings) runFromMenu("settings", () => openSettings(null, session.getSnapshot().presentation)); }
    else runFromMenu("quit", quitAfterSaving);
  };
  const searchKey = (e: KeyboardEvent) => {
    // A held Remove activation must not act again after focus returns.
    if (e.key === "Enter" && e.repeat) {
      e.preventDefault();
      return;
    }
    if (e.key === "Enter" && !e.shiftKey) onEmptyEnter?.();
    if (e.key === "ArrowDown" && (results.length === 0 || !showCitations) && focusChoices()) {
      e.preventDefault();
    } else if (!showCitations && e.key === "Enter") {
      // Choice screens have no citation action in Search.
      e.preventDefault();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      cancelCopy();
      followKeyboard.current = true; pointer.forgetClick();
      const step = e.key === "ArrowDown" ? 1 : -1;
      setActive(Math.min(Math.max(session.getSnapshot().active + step, 0), Math.max(results.length - 1, 0)));
    } else if (e.key === "Enter" && e.shiftKey) {
      e.preventDefault();
      toggleHighlighted();
    } else if (e.key === "Enter") {
      e.preventDefault();
      copyChosen();
    }
  };
  const lastShowing = useRef<number | null>(null);
  const keyDown = (e: KeyboardEvent) => {
    pointer.resetHover();
    // A key that hides the window releases over another one, so WebKitGTK calls
    // the next press of it a repeat. There a repeat counts only within one
    // showing: the web view cannot see that release, and a key held through a
    // reopening costs one hide, where a dead Esc would cost every press.
    const showing = session.getSnapshot().visibility;
    const repeated = e.repeat && (currentPlatform() !== "linux" || lastShowing.current === null || lastShowing.current === showing);
    lastShowing.current = showing;
    // A click on a blank part of the picker, or a control that went away,
    // leaves focus on nothing. The key goes home with its native effect, so
    // the text it types lands in Search. Focus is not moved before a key:
    // that would clear a selection made with the pointer.
    const lost = focusLost() && !overlayOpen();
    if (lost) focusHome();
    if (composingKey(e) || e.key === "Process") return;
    if (e.key === "Escape") {
      // One press, one step: holding Esc must not walk down to hiding.
      if (repeated || closeNearestLayer(e)) consumeKey(e);
      return;
    }
    const target = lost ? document.activeElement : e.target;
    const command = commandFor(e, target);
    if (command === "settings" && !openSettings) return;
    if (command) {
      consumeKey(e);
      if (!repeated) run(command, e);
    } else if (target === input.current) searchKey(e);
  };
  useKeyOwner({ keyDown, home: focusHome });
  return { run };
}
