// One playground board: a canvas scenario mounted as the canvas does, with an
// optional variant over it and the click-to-comment layer (docs/design.md, "Playground").
import { mount } from "../main";
import { type Comment, loadComments, saveComment } from "./comments";

const query = new URLSearchParams(location.search);
const where = {
  scenario: query.get("scenario") ?? "results", theme: query.get("theme") ?? "light",
  platform: query.get("platform") ?? "macos", text: Number(query.get("text") ?? 100), variant: query.get("variant") ?? "base",
};

const variants = import.meta.glob("../variants/*.css");
await variants[`../variants/${where.variant}.css`]?.();
// At 100 % a scenario keeps its own text size (text-80, text-120).
mount(document.getElementById("host")!, { scenario: where.scenario, theme: where.theme, ...where.text !== 100 && { textZoom: where.text / 100 } });
// A variant's rules win: its style moves after <body>, past the picker's own inline
// styles. Vite still updates the same element when the file is saved.
const variant = document.querySelector(`style[data-vite-dev-id$="/canvas/variants/${CSS.escape(where.variant)}.css"]`);
if (variant) document.documentElement.append(variant);

// Pins and the hover outline sit outside <body>, above everything, and never take a click.
const layer = document.createElement("div");
const hover = document.createElement("div");
const crosshair = document.createElement("style");
crosshair.textContent = "* { cursor: crosshair !important; }";
layer.style.cssText = "position: fixed; inset: 0; pointer-events: none; z-index: 2147483647;";
hover.style.cssText = "position: fixed; display: none; pointer-events: none; z-index: 2147483647; outline: 2px solid #0a84ff; outline-offset: 1px; border-radius: 3px;";
document.documentElement.append(layer, hover);

let commenting = false;
let pinned: Comment[] = [];

/** The shortest selector that finds this element alone, built from what the code names. */
function selectorOf(target: Element): string {
  const parts: string[] = [];
  let node: Element | null = target;
  for (; node && node !== document.body && node !== document.documentElement; node = node.parentElement) {
    if (node.id) { parts.unshift(`#${CSS.escape(node.id)}`); break; }
    const quoted = (value: string) => `"${value.replace(/["\\]/g, "\\$&")}"`;
    const label = node.getAttribute("aria-label"), role = node.getAttribute("role");
    let part = node.localName + (label ? `[aria-label=${quoted(label)}]` : role ? `[role=${quoted(role)}]` : "")
      + [...node.classList].slice(0, 2).map((name) => `.${CSS.escape(name)}`).join("");
    const siblings = [...node.parentElement?.children ?? []];
    if (siblings.filter((sibling) => sibling.matches(part)).length > 1) {
      const kind = node.localName;
      part += `:nth-of-type(${siblings.filter((sibling) => sibling.localName === kind).indexOf(node) + 1})`;
    }
    parts.unshift(part);
    if (document.querySelectorAll(parts.join(" > ")).length === 1) return parts.join(" > ");
  }
  if (node === document.body) parts.unshift("body");
  return parts.join(" > ");
}

function find(selector: string): Element | null {
  try { return document.querySelector(selector); } catch { return null; }
}

/** Each open comment on this board: a numbered pin and an outline on its element. */
function draw() {
  layer.replaceChildren(...pinned.flatMap((comment) => {
    const box = find(comment.selector)?.getBoundingClientRect();
    if (!box || (!box.width && !box.height)) return [];
    const frame = document.createElement("div");
    frame.style.cssText = `position: absolute; left: ${box.left}px; top: ${box.top}px; width: ${box.width}px; height: ${box.height}px; outline: 2px dashed #0a84ff; border-radius: 3px;`;
    const pin = document.createElement("span");
    pin.textContent = String(comment.id);
    pin.title = comment.comment;
    pin.style.cssText = `position: absolute; left: ${Math.max(0, box.right - 10)}px; top: ${Math.max(0, box.top - 10)}px; min-width: 20px; height: 20px; padding: 0 5px; box-sizing: border-box; border-radius: 10px; background: #0a84ff; color: #fff; font: 600 11px/20px system-ui, sans-serif; text-align: center; pointer-events: auto; box-shadow: 0 1px 3px rgb(0 0 0 / .3);`;
    return [frame, pin];
  }));
}

async function refresh() {
  pinned = (await loadComments()).filter((comment) => !comment.done && comment.scenario === where.scenario && comment.variant === where.variant);
  draw();
}

// In comment mode a click names the element instead of pressing it. The controls act on
// pointer events and menus close on an outside press, so every press is held back.
const hold = (event: Event) => {
  if (!commenting) return;
  event.preventDefault();
  event.stopImmediatePropagation();
};
for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "dblclick", "contextmenu", "auxclick"]) addEventListener(type, hold, true);
addEventListener("click", (event) => {
  if (!commenting || !(event.target instanceof Element)) return;
  hold(event);
  const selector = selectorOf(event.target);
  const comment = prompt(`Comment on ${selector}`)?.trim();
  if (comment) void saveComment({ ...where, selector, comment });
}, true);
addEventListener("pointermove", (event) => {
  const box = commenting && event.target instanceof Element ? event.target.getBoundingClientRect() : null;
  hover.style.display = box ? "block" : "none";
  if (box) Object.assign(hover.style, { left: `${box.left}px`, top: `${box.top}px`, width: `${box.width}px`, height: `${box.height}px` });
}, true);
addEventListener("message", (event) => {
  if (event.origin !== location.origin || event.data?.playground !== "commenting") return;
  commenting = event.data.on === true;
  if (commenting) document.head.append(crosshair);
  else { crosshair.remove(); hover.style.display = "none"; }
});

// Elements move as the board opens menus, scrolls or reflows; the pins follow.
setInterval(draw, 400);
import.meta.hot?.on("playground:comments", () => void refresh());
void refresh();
parent.postMessage({ playground: "ready" }, location.origin);
