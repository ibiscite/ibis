import { useEffect, useLayoutEffect, useRef, type RefObject } from "react";

export type FitPicker = (height: number | null, animate: boolean) => Promise<unknown>;

/** Size follows content (docs/design.md, C10). While `compact`, ask for the
 * content's own height; otherwise, or while a menu or dialog needs the room,
 * ask for the results height. The window side keeps the top edge, grows only
 * while open and animates unless motion is reduced. */
export function useFitWindow(main: RefObject<HTMLElement | null>, compact: boolean, fit: FitPicker | undefined, onShown?: (handler: () => void) => Promise<() => void>) {
  const sent = useRef<string | null>(null);
  const measure = useRef(() => {});
  measure.current = () => {
    const node = main.current;
    if (!node || !fit) return;
    const overlay = document.querySelector(".menu-popover, [role='dialog']") !== null;
    const footer = node.querySelector(":scope > footer");
    // Compact content pushes the footer down with an auto margin; without it
    // the main element is exactly as tall as its content.
    const spare = footer ? parseFloat(getComputedStyle(footer).marginBlockStart) || 0 : 0;
    const height = compact && !overlay ? Math.ceil(node.offsetHeight - spare) : null;
    const key = `${height}`;
    if (sent.current === key) return;
    sent.current = key;
    const animate = !window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    void fit(height, animate).catch(() => { sent.current = null; });
  };
  const resized = useRef<ResizeObserver | null>(null);
  // Watch the parts this render shows; observing a part twice is harmless.
  const watch = () => {
    for (const part of main.current?.querySelectorAll(":scope > *, :scope > .picker-content > *") ?? []) resized.current?.observe(part);
  };
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
  }, [onShown]);
}
