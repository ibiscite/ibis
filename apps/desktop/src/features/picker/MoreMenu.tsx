import type React from "react";
import { useEffect, useRef } from "react";
import { pickerText } from "../../messages";

/** A small menu: arrow keys move, Escape or a click outside closes it. */
export function MoreMenu({ children, onClose }: { children: React.ReactNode; onClose: (refocus: boolean) => void }) {
  const menu = useRef<HTMLDivElement>(null);
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
