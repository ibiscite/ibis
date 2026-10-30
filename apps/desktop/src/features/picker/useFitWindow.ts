import { useCallback, useEffect, useLayoutEffect, useRef, type RefObject } from "react";

export type FitPicker = (height: number | null, animate: boolean) => Promise<unknown>;

function contentHeight(node: HTMLElement) {
  // Measure natural layout synchronously, then restore the visible frame before
  // paint. A short viewport must not become its own requested content height.
  const previous = node.style.cssText;
  try {
    node.style.height = "auto";
    node.style.minHeight = "0";
    const footer = node.querySelector(":scope > footer");
    const spare = footer ? parseFloat(getComputedStyle(footer).marginBlockStart) || 0 : 0;
    const middle = node.querySelector<HTMLElement>(":scope > .picker-content");
    const overflow = middle && /auto|scroll|hidden|clip/.test(getComputedStyle(middle).overflowY)
      ? Math.max(0, middle.scrollHeight - middle.clientHeight) : 0;
    return Math.ceil(Math.max(node.offsetHeight, node.scrollHeight) + overflow - spare);
  } finally { node.style.cssText = previous; }
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
    if (!node || !fit) return;
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
  // Watch the parts this render shows; observing a part twice is harmless.
  const watch = useCallback(() => {
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
