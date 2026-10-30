import { useCallback, useEffect, useLayoutEffect, useRef, type RefObject } from "react";

export type FitPicker = (height: number | null, animate: boolean) => Promise<unknown>;

function contentHeight(node: HTMLElement) {
  // Measure natural layout synchronously, then restore the visible frame before
  // paint. A short viewport must not become its own requested content height.
  const previous = node.style.cssText;
  const middle = node.querySelector<HTMLElement>(":scope > .picker-content");
  const previousMiddle = middle?.style.cssText ?? "";
  const scrolled = [node, ...node.querySelectorAll<HTMLElement>("*")]
    .filter((part) => part.scrollTop || part.scrollLeft)
    .map((part) => ({ part, top: part.scrollTop, left: part.scrollLeft }));
  try {
    node.style.height = "auto";
    node.style.minHeight = "0";
    // Size containment makes an auto-height middle empty. Its children must
    // contribute their natural height before we read the picker's geometry.
    if (middle) {
      middle.style.containerType = "normal";
      middle.style.flex = "none";
    }
    const footer = node.querySelector(":scope > footer");
    const spare = footer ? parseFloat(getComputedStyle(footer).marginBlockStart) || 0 : 0;
    const overflow = middle && /auto|scroll|hidden|clip/.test(getComputedStyle(middle).overflowY)
      ? Math.max(0, middle.scrollHeight - middle.clientHeight) : 0;
    return Math.ceil(Math.max(node.offsetHeight, node.scrollHeight) + overflow - spare);
  } finally {
    if (middle) middle.style.cssText = previousMiddle;
    node.style.cssText = previous;
    // Expanding the live scrollport can clamp its offsets. Restore only after
    // the bounded layout is back, without moving focus or the text selection.
    for (const { part, top, left } of scrolled) {
      part.scrollTop = top;
      part.scrollLeft = left;
    }
  }
}

/** Size follows content (docs/design.md, C10). While `compact`, ask for the
 * content's own height; results and explicit dialogs can request more room.
 * Menus fit within the existing viewport and never expand the picker. Native
 * sizing keeps the top edge, grows only while open and respects reduced motion. */
export function useFitWindow(main: RefObject<HTMLElement | null>, compact: boolean, fit: FitPicker | undefined, onShown?: (handler: () => void) => Promise<() => void>) {
  const sent = useRef<string | null>(null);
  const request = useRef(0);
  const measure = useRef(() => {});
  measure.current = () => {
    const node = main.current;
    if (!node) return;
    if (node.clientHeight > 0) {
      const middle = node.querySelector(":scope > .picker-content");
      const edges = [...node.children].filter((part) => part !== middle).reduce((height, part) => {
        const style = getComputedStyle(part);
        // The compact footer's auto margin is spare room, not a fixed edge.
        const margin = part.tagName === "FOOTER" ? 0 : (parseFloat(style.marginBlockStart) || 0) + (parseFloat(style.marginBlockEnd) || 0);
        return height + (part as HTMLElement).offsetHeight + margin;
      }, 0);
      const control = node.querySelector<HTMLElement>(".search-bar input")?.offsetHeight ?? 0;
      node.toggleAttribute("data-scroll-page", edges + control > node.clientHeight);
    }
    if (!fit) return;
    const dialog = document.querySelector("[role='dialog']:not(.menu-popover)") !== null;
    const height = compact && !dialog ? contentHeight(node) : null;
    const key = `${height}`;
    if (sent.current === key) return;
    sent.current = key;
    const animate = !window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    const operation = ++request.current;
    void fit(height, animate).catch(() => { if (request.current === operation) sent.current = null; });
  };
  const resized = useRef<ResizeObserver | null>(null);
  // Fallback children keep their intrinsic size; observe the viewport too so
  // growing it can restore the bounded middle without a content rerender.
  const watch = useCallback(() => {
    if (main.current) resized.current?.observe(main.current);
    for (const part of main.current?.querySelectorAll(":scope > *, :scope > .picker-content > *") ?? []) resized.current?.observe(part);
  }, [main]);
  useLayoutEffect(() => {
    measure.current();
    watch();
  });
  useEffect(() => {
    let current = true;
    const update = () => { if (current) measure.current(); };
    resized.current = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(update);
    watch();
    // Menus and dialogs open in their own layer, outside the picker's tree.
    const layers = new MutationObserver(update);
    layers.observe(document.body, { childList: true });
    const textSize = new MutationObserver(update);
    textSize.observe(document.documentElement, { attributes: true, attributeFilter: ["data-text-size"] });
    const fonts = document.fonts;
    void fonts?.ready.then(update).catch(() => undefined);
    fonts?.addEventListener("loadingdone", update);
    fonts?.addEventListener("loadingerror", update);
    // Each opening starts again from the compact height.
    const shown = onShown?.(() => { if (current) { sent.current = null; update(); } });
    return () => {
      current = false;
      textSize.disconnect();
      fonts?.removeEventListener("loadingdone", update);
      fonts?.removeEventListener("loadingerror", update);
      resized.current?.disconnect();
      resized.current = null;
      layers.disconnect();
      void shown?.then((stop) => stop()).catch(() => undefined);
    };
  }, [onShown, watch]);
}
