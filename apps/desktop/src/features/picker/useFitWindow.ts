import { useCallback, useEffect, useLayoutEffect, useRef, type RefObject } from "react";

export type FitPicker = (height: number | null, animate: boolean) => Promise<unknown>;

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
    const footer = node.querySelector(":scope > footer");
    // Compact content pushes the footer down with an auto margin; without it
    // the main element is exactly as tall as its content.
    const spare = footer ? parseFloat(getComputedStyle(footer).marginBlockStart) || 0 : 0;
    const height = compact && !dialog ? Math.ceil(node.offsetHeight - spare) : null;
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
    const update = () => measure.current();
    resized.current = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(update);
    watch();
    // Menus and dialogs open in their own layer, outside the picker's tree.
    const layers = new MutationObserver(update);
    layers.observe(document.body, { childList: true });
    // Each opening starts again from the compact height.
    const shown = onShown?.(() => { sent.current = null; update(); });
    return () => {
      resized.current?.disconnect();
      resized.current = null;
      layers.disconnect();
      void shown?.then((stop) => stop()).catch(() => undefined);
    };
  }, [onShown, watch]);
}
