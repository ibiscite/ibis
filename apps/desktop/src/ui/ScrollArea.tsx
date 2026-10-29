import { useEffect, type RefObject } from "react";
import "./ScrollArea.css";

/** Marks the edges of a `.scroll-area` that have more content beyond them; ScrollArea.css fades them. */
export function markScrollEdges(node: HTMLElement) {
  node.toggleAttribute("data-more-above", node.scrollTop > 0);
  node.toggleAttribute("data-more-below", node.scrollHeight - node.clientHeight - node.scrollTop >= 1);
  const focused = node.querySelector<HTMLElement>("[data-focused]") ??
    (node.contains(document.activeElement) && document.activeElement !== node ? document.activeElement as HTMLElement : null);
  const bounds = node.getBoundingClientRect();
  const scale = node.offsetHeight && bounds.height ? bounds.height / node.offsetHeight : 1;
  const top = bounds.top + node.clientTop * scale, bottom = top + node.clientHeight * scale;
  const row = focused?.getBoundingClientRect();
  if (row && row.bottom > top && row.top < bottom) {
    // Mask only the free space outside the whole active row, including its fill.
    node.style.setProperty("--scroll-fade-above-space", `${Math.max(0, Math.floor((row.top - top) / scale))}px`);
    node.style.setProperty("--scroll-fade-below-space", `${Math.max(0, Math.floor((bottom - row.bottom) / scale))}px`);
  } else {
    node.style.removeProperty("--scroll-fade-above-space");
    node.style.removeProperty("--scroll-fade-below-space");
  }
}

/** Keeps a scroll area's edge marks current as it scrolls, resizes or its content changes. */
export function useScrollEdges(ref: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    let current = true;
    const mark = () => { if (current) markScrollEdges(node); };
    const resized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(mark);
    resized?.observe(node);
    // Content only: the marks are attributes, so setting them never re-runs this.
    const content = new MutationObserver(mark);
    content.observe(node, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ["data-focused"] });
    node.addEventListener("scroll", mark, { passive: true });
    node.addEventListener("focusin", mark);
    node.addEventListener("focusout", mark);
    mark();
    return () => {
      current = false;
      resized?.disconnect();
      content.disconnect();
      node.removeEventListener("scroll", mark);
      node.removeEventListener("focusin", mark);
      node.removeEventListener("focusout", mark);
    };
  }, [ref]);
}
