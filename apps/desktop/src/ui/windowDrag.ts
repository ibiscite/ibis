/** The attribute that makes an element move the window (Tauri's drag script). */
export const DRAG_REGION = "data-tauri-drag-region";

const CLICKABLE = new Set(["A", "BUTTON", "INPUT", "SELECT", "TEXTAREA", "LABEL", "SUMMARY"]);
const INTERACTIVE_ROLES = new Set(["button", "link", "menuitem", "tab", "checkbox", "radio", "switch", "option"]);
const clickable = (node: HTMLElement) => CLICKABLE.has(node.tagName)
  || (node.hasAttribute("contenteditable") && node.getAttribute("contenteditable") !== "false")
  || (node.hasAttribute("tabindex") && node.getAttribute("tabindex") !== "-1")
  || INTERACTIVE_ROLES.has(node.getAttribute("role") ?? "");

/** Whether a press on `target` moves the window, decided exactly as Tauri's own
 * drag script decides, so the app and the native move always agree. Walking up
 * from the pressed element: a control without the attribute blocks, "false"
 * blocks, "deep" moves from anywhere inside, and a bare or "true" region moves
 * only when it is pressed itself. */
export function startsWindowDrag(target: EventTarget | null): boolean {
  for (let node = target instanceof Element ? target : null; node; node = node.parentElement) {
    if (!(node instanceof HTMLElement)) continue;
    const region = node.getAttribute(DRAG_REGION);
    if (region === null) {
      if (clickable(node)) return false;
    } else if (region === "false") return false;
    else if (region === "deep") return true;
    else if (region === "" || region === "true") return node === target;
  }
  return false;
}
