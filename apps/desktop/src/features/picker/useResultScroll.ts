import { useEffect, useRef, type RefObject } from "react";
import type { usePointerSelection } from "../../pointerSelection";
import type { Choice } from "./choice";
import type { useCitationOutput } from "./useCitationOutput";

export function useResultScroll({ input, active, restored, pointer, selectedSize, citationProblem, formatted, format, searchFocused }: {
  input: RefObject<HTMLInputElement | null>;
  active: number;
  restored: number;
  pointer: ReturnType<typeof usePointerSelection>;
  selectedSize: number;
  citationProblem: string | null;
  formatted: ReturnType<typeof useCitationOutput>["formatted"];
  format: Choice;
  searchFocused: boolean;
}) {
  // A restored search is selected once it is back in the box, so typing replaces it.
  // biome-ignore lint/correctness/useExhaustiveDependencies: only a restored search is selected; Search's ref is stable.
  useEffect(() => {
    if (restored > 0) input.current?.select();
  }, [restored]);

  // Keep the highlighted result visible while moving with the arrow keys.
  // A row clicked is already under the pointer; scrolling would move it away.
  // Selection/preview reflow follows the highlight only while Search has focus.
  const previousActive = useRef(active);
  const followKeyboard = useRef(true);
  const resultArea = useRef<HTMLDivElement>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the conditional values are triggers: while references are chosen, a preview, format or focus change can move the row, so it is revealed again.
  useEffect(() => {
    const moved = previousActive.current !== active;
    previousActive.current = active;
    const from = pointer.cameFrom(active);
    if (from !== "other") {
      if (from === "click") followKeyboard.current = false;
      return;
    }
    if (moved) followKeyboard.current = true;
    if (!moved && (citationProblem || selectedSize === 0 || document.activeElement !== input.current)) return;
    document.getElementById(`result-${active}`)?.scrollIntoView?.({ block: "nearest" });
  }, [active, pointer.cameFrom, selectedSize, citationProblem,
    selectedSize > 0 ? formatted : null, selectedSize > 0 ? format : null, selectedSize > 0 ? searchFocused : null]);

  // Content changes preserve Search's keyboard highlight. A real viewport
  // resize must also leave the selected title readable while Page is edited.
  // biome-ignore lint/correctness/useExhaustiveDependencies: each highlight owns its resize observer; Search's ref is stable.
  useEffect(() => {
    const area = resultArea.current;
    if (!area) return;
    const scrollers = [area, area.parentElement].filter((node): node is HTMLElement => node !== null);
    const reveal = (viewport = false) => {
      // A refusal owns the middle viewport. Read the committed DOM so even a
      // queued observer cannot undo its reveal before effects are refreshed,
      // or scroll past the person's newer reading position. Explicit arrows
      // still reveal the highlighted row through the effect above.
      if (area.parentElement?.querySelector(":scope > .notices > .citation-problem")) return;
      if (!viewport && (!followKeyboard.current || document.activeElement !== input.current)) return;
      const row = document.getElementById(`result-${active}`);
      if (!row) return;
      for (const scroller of scrollers) {
        if (scroller.clientHeight <= 0 || !/auto|scroll/.test(getComputedStyle(scroller).overflowY)) continue;
        const top = scroller.getBoundingClientRect().top + scroller.clientTop;
        const bottom = top + scroller.clientHeight;
        const whole = row.getBoundingClientRect();
        const item = whole.height > scroller.clientHeight
          ? row.querySelector(".result-title")?.getBoundingClientRect() ?? whole : whole;
        // A tall row already filling the scrollport cannot be shown any fuller.
        if (item.top <= top && item.bottom >= bottom) continue;
        if (item.bottom > bottom) scroller.scrollTop += Math.ceil(item.bottom - bottom);
        else if (item.top < top) scroller.scrollTop += Math.floor(item.top - top);
      }
    };
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => reveal());
    for (const scroller of scrollers) observer?.observe(scroller);
    const resized = () => reveal(true);
    window.addEventListener("resize", resized);
    return () => { observer?.disconnect(); window.removeEventListener("resize", resized); };
  }, [active]);

  return { resultArea, followKeyboard };
}
