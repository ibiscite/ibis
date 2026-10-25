import { useEffect, type RefObject } from "react";
import "./ScrollArea.css";

/** Marks the edges of a `.scroll-area` that have more content beyond them; ScrollArea.css fades them. */
export function markScrollEdges(node: HTMLElement) {
  node.toggleAttribute("data-more-above", node.scrollTop > 0);
  node.toggleAttribute("data-more-below", node.scrollHeight - node.clientHeight - node.scrollTop >= 1);
}

/** Keeps a scroll area's edge marks current as it scrolls, resizes or its content changes. */
export function useScrollEdges(ref: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const mark = () => markScrollEdges(node);
    const resized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(mark);
    resized?.observe(node);
    // Content only: the marks are attributes, so setting them never re-runs this.
    const content = new MutationObserver(mark);
    content.observe(node, { childList: true, subtree: true, characterData: true });
    node.addEventListener("scroll", mark, { passive: true });
    mark();
    return () => {
      resized?.disconnect();
      content.disconnect();
      node.removeEventListener("scroll", mark);
    };
  }, [ref]);
}
