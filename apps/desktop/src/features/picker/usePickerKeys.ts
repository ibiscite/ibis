import type { KeyboardEvent } from "react";
import type { PickerApi } from "../../App";
import { focusChoices } from "../../ui/ChoiceList";
import type { FormatCommand } from "./FormatMenu";
import type { ToolbarMenuCommand } from "../../ui/ToolbarMenu";
import { composing, isFormatShortcut, isKeysShortcut, isMenuShortcut, isPinShortcut, isPreviewShortcut, isQuitShortcut, isSettingsShortcut, isSourceShortcut, isChooseSourceShortcut } from "./shortcuts";
import type { usePickerSession } from "./usePickerSession";
import type { usePickerMenus } from "./usePickerMenus";
import type { usePickerSource } from "./usePickerSource";
import type { useLibrarySearch } from "./useLibrarySearch";
import type { useCitationRows } from "./useCitationRows";
import type { usePickerActions } from "./usePickerActions";
import type { usePointerSelection } from "../../pointerSelection";
import type { useOperations } from "./useOperations";

export function usePickerKeys({
  api, pickerSession, menus, source, results, showCitations, copyChosen, actions, pointer, followKeyboard,
  togglePastePreview, quitAfterSaving, perform,
}: {
  api: PickerApi;
  pickerSession: Pick<ReturnType<typeof usePickerSession>, "session" | "cancelCopy" | "setActive">;
  menus: ReturnType<typeof usePickerMenus>;
  source: Pick<ReturnType<typeof usePickerSource>, "chooseSources" | "openSource">;
  results: ReturnType<typeof useLibrarySearch>["results"];
  showCitations: boolean;
  copyChosen: ReturnType<typeof useCitationRows>["copyChosen"];
  actions: Pick<ReturnType<typeof usePickerActions>, "escapeStep" | "toggleHighlighted">;
  pointer: ReturnType<typeof usePointerSelection>;
  followKeyboard: { current: boolean };
  togglePastePreview: () => void;
  quitAfterSaving: () => Promise<void>;
  perform: ReturnType<typeof useOperations>["perform"];
}) {
  const { session, cancelCopy, setActive } = pickerSession;
  const { formatButtonRef, menuOpen, keysOpen, togglePin, openKeys, closeMenu, openMore, runFromMenu } = menus;
  const { chooseSources, openSource } = source;
  const { escapeStep, toggleHighlighted } = actions;
  const { openSettings } = api;
  const onKeyDownCapture = (e: KeyboardEvent<HTMLElement>) => {
    pointer.resetHover();
    if (composing(e)) return;
    // Format owns its modal exit and restores Search before running a
    // command. Let its capture handler consume these keys first.
    if (e.target instanceof Element && (e.target.closest(".toolbar-menu-popover") ||
      formatButtonRef.current?.contains(e.target) && formatButtonRef.current.getAttribute("aria-expanded") === "true")) return;
    if (isChooseSourceShortcut(e)) {
      e.preventDefault(); e.stopPropagation();
      if (!e.repeat) chooseSources();
    } else if (isSourceShortcut(e)) {
      e.preventDefault();
      e.stopPropagation();
      if (!e.repeat) openSource(false, e.altKey);
    }
  };
  const onKeyDown = (e: KeyboardEvent<HTMLElement>) => {
    if (composing(e)) return;
    if (isFormatShortcut(e)) {
      e.preventDefault();
      if (!e.repeat && !menuOpen && !keysOpen) formatButtonRef.current?.click();
    } else if (isPinShortcut(e)) {
      e.preventDefault();
      if (!e.repeat) togglePin();
    } else if (isPreviewShortcut(e)) {
      e.preventDefault();
      if (!e.repeat) togglePastePreview();
    } else if (isKeysShortcut(e)) {
      e.preventDefault();
      openKeys();
    } else if (e.key === "Escape") {
      e.preventDefault();
      escapeStep();
    } else if (isMenuShortcut(e)) {
      e.preventDefault();
      if (menuOpen) closeMenu(true);
      else openMore();
    } else if (isSettingsShortcut(e) && openSettings) {
      e.preventDefault();
      if (!e.repeat) runFromMenu("settings", () => openSettings(null));
    } else if (isQuitShortcut(e)) {
      e.preventDefault();
      void perform("quit", quitAfterSaving);
    }
  };
  const onSearchKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (composing(e)) return;
    // A held Remove activation must not act again after focus returns.
    if (e.key === "Enter" && e.repeat) {
      e.preventDefault();
      return;
    }
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
  const onFormatCommand = (command: FormatCommand): ToolbarMenuCommand => {
    const visibility = session.getSnapshot().visibility;
    return {
      isCurrent: () => visibility === session.getSnapshot().visibility && session.getSnapshot().hiddenAt === null,
      run: () => {
        if (command === "more") openMore();
        else if (command === "keys") openKeys();
        else if (command === "pin") togglePin();
        else if (command === "preview") togglePastePreview();
        else if (command === "quit") void perform("quit", quitAfterSaving);
        else if (command === "chooseSource") chooseSources();
        else if (command === "source" || command === "revealSource") openSource(false, command === "revealSource");
      },
    };
  };
  return { onKeyDownCapture, onKeyDown, onSearchKeyDown, onFormatCommand };
}
