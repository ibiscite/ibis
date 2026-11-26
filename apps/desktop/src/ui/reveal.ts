import { useLayoutEffect, type RefObject } from "react";
import { visibleBox } from "./region";

/** What a reveal shows: `find` returns the element once it is drawn and ready
 * (null while it is missing or still loading); `focus`, when given, names what
 * takes the keyboard once it shows (null: the element itself). */
export type RevealTarget = {
  find: (scroller: HTMLElement) => HTMLElement | null;
  focus?: (target: HTMLElement) => HTMLElement | null;
};

/** How long a reveal waits for its target to be drawn and ready before it gives up. */
export const REVEAL_WAIT_MS = 10_000;

const running = new WeakMap<HTMLElement, () => void>();

/** One reveal per scroll owner (decision 0062): a newer one ends the last. It
 * waits until the view says its target is ready, then brings the target to the
 * top of what the person sees of `scroller` (under a sticky title, never behind
 * it) and hands it the keyboard. From then on it keeps the target where it
 * landed, however late content above it arrives, as scroll anchoring would,
 * until the person scrolls (by any means), points, types or moves focus, or a
 * newer reveal starts. The key or click that asked for it never counts. DOM
 * and size changes wake it, never a poll. Returns its cancel. */
export function reveal(scroller: HTMLElement, { find, focus }: RevealTarget): () => void {
  running.get(scroller)?.();
  let phase: "waiting" | "holding" | "done" = "waiting";
  let landed = 0, focused: HTMLElement | null = null;
  // The key or click that asked for this reveal is still on its way to the
  // document's listeners below: it is not the person moving on.
  const asking = typeof window.event === "object" ? window.event : undefined;
  const offset = (target: HTMLElement) => target.getBoundingClientRect().top - visibleBox(scroller).top;
  const place = (target: HTMLElement) => {
    const delta = offset(target) - landed;
    if (Math.abs(delta) >= 1) scroller.scrollTop += delta;
  };
  const take = (target: HTMLElement) => {
    if (!focus) return;
    const control = focus(target);
    if (!control) target.tabIndex = -1;
    focused = control ?? target;
    focused.focus({ preventScroll: true });
  };
  const check = () => {
    if (phase === "done") return;
    const target = find(scroller);
    if (!target || !scroller.contains(target)) return;
    if (phase === "holding") {
      place(target);
      // The target was drawn again (its list finished reading): the keyboard follows it.
      if (focused && !focused.isConnected) take(target);
      return;
    }
    landed = 0;
    place(target);
    // Near the page's end it lands as far as the page scrolls.
    landed = offset(target);
    phase = "holding";
    window.clearTimeout(timer);
    take(target);
  };
  const changed = new MutationObserver(check);
  changed.observe(scroller, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ["disabled", "hidden", "checked", "aria-busy", "class", "style"] });
  // Sizes change without the DOM changing too (a font or picture arriving).
  const resized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(check);
  resized?.observe(scroller);
  for (const child of scroller.children) resized?.observe(child);
  // Our own focus and scrolling never count as the person moving on.
  const moved = (event: Event) => { if (event !== asking && (phase === "waiting" || event.target !== focused)) stop(); };
  const key = (event: KeyboardEvent) => { if (event !== asking && !event.repeat) stop(); };
  const acted = (event: Event) => { if (event !== asking) stop(); };
  // Scrolling by any means (a scroll bar, a screen reader, a script) moves the
  // target off where it landed; the reveal's own writes and the late content it
  // answers leave it there. The person's scroll ends the hold.
  const scrolled = () => {
    if (phase !== "holding") return;
    const target = find(scroller);
    if (target && Math.abs(offset(target) - landed) >= 1) stop();
  };
  const leave = ["pointerdown", "wheel", "touchmove"] as const;
  document.addEventListener("focusin", moved);
  for (const type of leave) document.addEventListener(type, acted, { passive: true });
  document.addEventListener("keydown", key);
  scroller.addEventListener("scroll", scrolled, { passive: true });
  const timer = window.setTimeout(() => { if (phase === "waiting") stop(); }, REVEAL_WAIT_MS);
  function stop() {
    if (phase === "done") return;
    phase = "done";
    changed.disconnect();
    resized?.disconnect();
    window.clearTimeout(timer);
    document.removeEventListener("focusin", moved);
    for (const type of leave) document.removeEventListener(type, acted);
    document.removeEventListener("keydown", key);
    scroller.removeEventListener("scroll", scrolled);
    if (running.get(scroller) === stop) running.delete(scroller);
  }
  running.set(scroller, stop);
  check();
  return stop;
}

/** Reveal `target` in `scroller` while it is set; a new target is a new reveal. */
export function useReveal(scroller: RefObject<HTMLElement | null>, target: RevealTarget | null) {
  useLayoutEffect(() => {
    const box = scroller.current;
    if (!target || !box) return;
    return reveal(box, target);
  }, [scroller, target]);
}
