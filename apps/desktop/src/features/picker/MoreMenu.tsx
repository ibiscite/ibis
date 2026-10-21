import type React from "react";
import { useEffect, useLayoutEffect, useRef } from "react";
import { pickerText } from "../../messages";

/** A small menu: arrow keys move, Escape or a click outside closes it. */
export function MoreMenu({ children, onClose }: { children: React.ReactNode; onClose: (refocus: boolean) => void }) {
  const menu = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const node = menu.current!;
    let offset = 0;
    const fit = () => {
      const padding = parseFloat(getComputedStyle(node).paddingTop) || 0;
      const inset = Math.max(8, padding * 2);
      // Recover the anchor before our translation; footer wrapping and scrolling
      // can move it independently of the menu's own height.
      const naturalBottom = node.getBoundingClientRect().bottom + offset;
      let bottom = Math.min(naturalBottom, innerHeight - inset);
      const itemHeight = node.querySelector<HTMLElement>("[data-menu-item]")?.getBoundingClientRect().height ?? 0;
      if (bottom < inset + itemHeight + padding * 2) bottom = innerHeight - inset;
      offset = naturalBottom - bottom;
      node.style.setProperty("--more-menu-offset", `${offset}px`);
      node.style.maxHeight = `${Math.max(0, bottom - inset)}px`;
    };
    const refit = () => {
      fit();
      const active = document.activeElement;
      if (active instanceof HTMLElement && node.contains(active)) {
        const bounds = node.getBoundingClientRect();
        const item = active.getBoundingClientRect();
        const top = bounds.top + node.clientTop;
        const bottom = top + node.clientHeight;
        if (item.top < top) node.scrollTop += Math.floor(item.top - top);
        else if (item.bottom > bottom) node.scrollTop += Math.ceil(item.bottom - bottom);
      }
    };
    fit();
    // Watch both content/text changes and the footer that positions the trigger.
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(refit);
    observer?.observe(node);
    if (node.parentElement?.parentElement) observer?.observe(node.parentElement.parentElement);
    window.addEventListener("resize", refit);
    document.addEventListener("scroll", fit, true);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", refit);
      document.removeEventListener("scroll", fit, true);
    };
  }, []);
  useEffect(() => {
    menu.current?.querySelector<HTMLElement>("[data-menu-item]")?.focus();
    const outside = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!menu.current?.contains(target) && !menu.current?.parentElement?.contains(target)) onClose(false);
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, []);
  return (
    <div
      id="more-menu"
      role="menu"
      aria-label={pickerText.more}
      ref={menu}
      onKeyDown={(e) => {
        const items = [...(menu.current?.querySelectorAll<HTMLElement>("[data-menu-item]") ?? [])];
        const at = items.indexOf(document.activeElement as HTMLElement);
        const move = { ArrowDown: 1, ArrowUp: -1 }[e.key];
        if (move !== undefined) {
          e.preventDefault();
          items[(at + move + items.length) % items.length]?.focus();
        } else if (e.key === "Home" || e.key === "End") {
          e.preventDefault();
          items[e.key === "Home" ? 0 : items.length - 1]?.focus();
        } else if (e.key === "Tab") {
          // Leaving the menu returns to the search box, so keys keep working.
          e.preventDefault();
          onClose(false);
          document.getElementById("search")?.focus();
        }
      }}
    >
      {children}
    </div>
  );
}

export function MenuItem({ label, title, keys, checked, disabled, onSelect }: {
  label: string; title?: string; keys?: string; checked?: boolean; disabled?: boolean; onSelect: () => void;
}) {
  // A checkable item keeps the menuitem role for focus handling; aria-checked
  // is announced on menuitemcheckbox only, so the role changes with it. A
  // disabled item stays focusable so arrow keys still reach it.
  return (
    <button type="button" role={checked === undefined ? "menuitem" : "menuitemcheckbox"} aria-checked={checked}
      aria-disabled={disabled || undefined} data-menu-item tabIndex={-1} title={title}
      onClick={() => { if (!disabled) onSelect(); }}>
      <span>{label}</span>
      {keys && <kbd aria-hidden="true">{keys}</kbd>}
    </button>
  );
}
