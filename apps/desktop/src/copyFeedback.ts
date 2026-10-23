// What a copy says afterwards, and what a click on a result row does.
// Pure decisions, kept apart from the picker so they can be tested alone.
import type { Copied } from "./api";
import { copiedNoticeText } from "./messages";
import { platformOf, type Platform } from "./platform";

/**
 * After a copy: hide the picker and say "Copied" in the small notice window,
 * or stay open. A pinned picker stays beside the document and shows the
 * confirmation itself; warnings stay in the picker until dismissed.
 */
export function afterCopy(outcome: Copied, pinned: boolean): "hide-and-notify" | "stay" {
  return outcome.warnings.length === 0 && !pinned ? "hide-and-notify" : "stay";
}

export type RowClick = {
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  /** 1 for a single click, 2 for the second click of a double click. */
  detail: number;
  /** How far the pointer moved since it was pressed on this row, in pixels. */
  moved: number;
  /** The click landed on a button, link or field inside the row. */
  onControl: boolean;
  /** The user selected text in the row by dragging. */
  selectedText: boolean;
};

/** Beyond this distance a press and release is a drag, not a click. */
export const DRAG_DISTANCE = 4;

/** A single click selects; double-click copies. The platform command modifier
 * (or the existing Shift shortcut) toggles a citation. Controls and drags never act. */
export function rowClickAction(click: RowClick, mac = false): "select" | "copy" | "toggle" | "none" {
  if (click.onControl || click.selectedText || click.moved > DRAG_DISTANCE || click.altKey) return "none";
  const command = mac ? click.metaKey && !click.ctrlKey : click.ctrlKey && !click.metaKey;
  if ((click.ctrlKey || click.metaKey) && !command) return "none";
  if (click.detail > 2) return "none";
  if (command || click.shiftKey) return click.detail > 1 ? "none" : "toggle";
  return click.detail === 2 ? "copy" : "select";
}

export type NoticePlatform = Platform;

export const noticePlatform: (platform: string) => NoticePlatform = platformOf;

/**
 * Draw the notice: a check mark, "Copied" and the copied text on one line.
 * Text goes in as text, never as markup. The status region exists before its
 * text arrives, so screen readers that follow other windows announce it
 * politely without the notice taking focus.
 */
export function renderNotice(root: HTMLElement, platform: NoticePlatform): (text: string) => void {
  root.replaceChildren();
  const notice = document.createElement("div");
  notice.className = "copied-notice";
  notice.dataset.platform = platform;
  notice.setAttribute("role", "status");
  notice.setAttribute("aria-live", "polite");
  notice.setAttribute("aria-atomic", "true");
  const source = document.createElement("span");
  source.className = "copied-notice-source";
  source.textContent = copiedNoticeText.source;
  const mark = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  mark.setAttribute("aria-hidden", "true");
  mark.setAttribute("viewBox", "0 0 20 20");
  mark.classList.add("copied-notice-mark");
  const tick = document.createElementNS("http://www.w3.org/2000/svg", "path");
  tick.setAttribute("d", "m5 10.5 3.2 3.2L15 6.5");
  mark.append(tick);
  const title = document.createElement("strong");
  const code = document.createElement("code");
  code.dir = "auto";
  root.append(notice);
  return (text: string) => {
    title.textContent = platform === "windows" ? copiedNoticeText.windowsTitle : copiedNoticeText.title;
    code.textContent = text;
    notice.replaceChildren(source, mark, title, code);
    // Restart the fade for a notice shown again while still visible.
    notice.classList.remove("showing", "leaving");
    void notice.offsetWidth;
    notice.classList.add("showing");
  };
}

/** Start the notice's fade out; the app takes the window away once it ends. */
export function leaveNotice(root: HTMLElement): void {
  root.querySelector(".copied-notice")?.classList.add("leaving");
}
