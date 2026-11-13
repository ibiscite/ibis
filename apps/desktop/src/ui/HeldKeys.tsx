import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { keyFromAria } from "../keys";
import { currentPlatform } from "../platform";
import { KeyCaps } from "./KeyCaps";
import "./HeldKeys.css";
import "./Overlay.css";

export const HOLD_MS = 1000;

/** Where a badge goes: beside its control (above, below, after, before), or, where
 * each of those would cover words or another control, inside the control's own box. */
type Side = "above" | "below" | "after" | "before" | "inside";
type Shown = { key: string; control: DOMRect; side?: Side; left?: number; top?: number };

/** Parts a badge may not cover: controls and words a person can see. */
const blocking = 'button, input:not([type="hidden"]), select, textarea, a[href], [role="option"], [role="menuitem"], [tabindex]:not([tabindex="-1"])';

/** The visible part of a box, clipped by every scrolling or clipping ancestor of its node. */
function visible(node: Element, rect: DOMRect): DOMRect | null {
  let { left, top, right, bottom } = rect;
  for (let part = node.parentElement; part && part !== document.body; part = part.parentElement) {
    const style = getComputedStyle(part);
    if (style.overflowX === "visible" && style.overflowY === "visible") continue;
    const clip = part.getBoundingClientRect();
    left = Math.max(left, clip.left); top = Math.max(top, clip.top); right = Math.min(right, clip.right); bottom = Math.min(bottom, clip.bottom);
  }
  return right - left > 1 && bottom - top > 1 ? new DOMRect(left, top, right - left, bottom - top) : null;
}

/** What is drawn on screen outside the badges: each control's box and each run of words. */
function occupied(): DOMRect[] {
  const boxes: DOMRect[] = [];
  for (const node of document.querySelectorAll(blocking)) {
    if (node.closest(".ui-held-key, [inert], [hidden], .visually-hidden")) continue;
    const box = visible(node, node.getBoundingClientRect());
    if (box) boxes.push(box);
  }
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let text = walker.nextNode(); text; text = walker.nextNode()) {
    const parent = text.parentElement;
    if (!text.textContent?.trim() || !parent || parent.closest(".ui-held-key, .visually-hidden")) continue;
    const style = getComputedStyle(parent);
    if (style.visibility === "hidden" || Number(style.opacity) === 0) continue;
    const range = document.createRange();
    range.selectNodeContents(text);
    // jsdom lays out no text.
    for (const rect of range.getClientRects?.() ?? []) {
      const box = visible(text.parentElement!, rect);
      if (box) boxes.push(box);
    }
  }
  return boxes;
}

const overlaps = (a: DOMRect, b: DOMRect) => Math.min(a.right, b.right) - Math.max(a.left, b.left) > 1 && Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 1;

/** The first place beside the control where the badge covers nothing and stays in
 * the window; else inside the control's own box, over its own label only. */
function place(control: DOMRect, width: number, height: number, taken: DOMRect[], rtl: boolean): Pick<Shown, "side" | "left" | "top"> {
  const gap = 4, maxLeft = innerWidth - width, maxTop = innerHeight - height;
  const centreX = Math.min(Math.max(control.left + control.width / 2 - width / 2, 0), Math.max(maxLeft, 0));
  const centreY = Math.min(Math.max(control.top + control.height / 2 - height / 2, 0), Math.max(maxTop, 0));
  const sides: [Side, number, number][] = [
    ["above", centreX, control.top - gap - height], ["below", centreX, control.bottom + gap],
    ["after", rtl ? control.left - gap - width : control.right + gap, centreY],
    ["before", rtl ? control.right + gap : control.left - gap - width, centreY],
  ];
  for (const [side, left, top] of sides) {
    const box = new DOMRect(left, top, width, height);
    if (left >= 0 && top >= 0 && left <= maxLeft && top <= maxTop && !taken.some((other) => overlaps(box, other))) return { side, left, top };
  }
  return { side: "inside", left: control.left, top: control.top };
}

/** Controls that show no key of their own, read when the hold completes. Only keys
 * that use the held key are named: one that works without it (a token's ↓) does
 * something else while ⌘ is down. */
function keyedControls(held: string): Shown[] {
  return [...document.querySelectorAll<HTMLElement>("[aria-keyshortcuts]")].flatMap((control) => {
    const box = control.getBoundingClientRect();
    const shortcut = control.getAttribute("aria-keyshortcuts")!.split(" ").find((keys) => keys.split("+").includes(held));
    if (!shortcut || control.querySelector("kbd") || !box.width || !box.height) return [];
    return [{ key: keyFromAria(shortcut), control: box }];
  });
}

/** Holding ⌘ (Ctrl elsewhere) alone for a second shows the key of every visible
 * action that uses it. Any other key, a click, letting go or leaving the window
 * ends it. It never takes focus or a click, and covers no words and no other
 * control: each badge sits beside its control, or inside it where no side is free. */
export function HeldKeys() {
  const [shown, setShown] = useState<Shown[]>([]);
  const badges = useRef<(HTMLSpanElement | null)[]>([]);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => {
    const held = currentPlatform() === "macos" ? "Meta" : "Control";
    // While held, rows numbered for ⌘1–9 show their number (Option.css).
    const end = () => { window.clearTimeout(timer.current); timer.current = undefined; setShown([]); delete document.documentElement.dataset.keysHeld; };
    const down = (event: KeyboardEvent) => {
      if (event.repeat) return;
      end();
      if (event.key === held) timer.current = window.setTimeout(() => { document.documentElement.dataset.keysHeld = ""; setShown(keyedControls(held)); }, HOLD_MS);
    };
    window.addEventListener("keydown", down, true);
    window.addEventListener("keyup", end, true);
    window.addEventListener("pointerdown", end, true);
    window.addEventListener("blur", end);
    return () => {
      end();
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("keyup", end, true);
      window.removeEventListener("pointerdown", end, true);
      window.removeEventListener("blur", end);
    };
  }, []);
  // Measured unplaced, then placed before they are painted, each clear of the ones before it.
  useLayoutEffect(() => {
    if (!shown.length || shown.every((badge) => badge.side)) return;
    const taken = occupied(), rtl = getComputedStyle(document.documentElement).direction === "rtl";
    setShown(shown.map((badge, i) => {
      const size = badges.current[i]?.getBoundingClientRect();
      const width = size?.width ?? 0, height = size?.height ?? 0;
      const placed = place(badge.control, width, height, taken, rtl);
      taken.push(placed.side === "inside" ? badge.control : new DOMRect(placed.left, placed.top, width, height));
      return { ...badge, ...placed };
    }));
  }, [shown]);
  return <>{shown.map(({ key, control, side, left, top }, i) => (
    <span className="ui-held-key ui-overlay" key={`${key}-${control.left}-${control.top}`} ref={(node) => { badges.current[i] = node; }} aria-hidden="true"
      data-side={side} style={side === "inside" ? { left, top, width: control.width, height: control.height } : { left, top }}><KeyCaps accelerator={key} /></span>
  ))}</>;
}
