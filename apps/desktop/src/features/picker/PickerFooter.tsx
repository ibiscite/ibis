import type React from "react";
import { useLayoutEffect, useRef, useState } from "react";
import type { LibraryStatus } from "../../api";
import { ariaKeys, keysFor } from "../../keys";
import { LibraryStatusLine } from "../../LibraryStatusLine";
import "./PickerFooter.css";
import { keyText, pickerText } from "../../messages";
import { currentPlatform } from "../../platform";
import { KeyHint } from "../../ui/KeyCaps";
import { Button } from "../../ui/Button";

/** What the keys do now; null where a key does nothing. */
export type KeyHints = { enter: string | null; shiftEnter: string | null; escape: string };

/** The footer: library status, what Enter (or, with Alt held, the other keys)
 * does now, and the More button with its menu. */
export function PickerFooter({ library, groupName, hints, altHeld, actions, more, pinned, onUnpin }: {
  library: LibraryStatus | null | undefined;
  groupName: (id: number) => string;
  hints: KeyHints;
  /** While Alt (⌥) is held, the footer shows the other keys instead of Enter. */
  altHeld: boolean;
  actions: { copy: () => void; add: () => void; escape: () => void; keys: () => void };
  /** The More button and its menu. */
  more: React.ReactNode;
  pinned: boolean;
  onUnpin: () => void;
}) {
  const bar = useRef<HTMLElement>(null);
  const hintsRow = useRef<HTMLElement>(null);
  const shortcut = useRef<HTMLButtonElement>(null);
  const pin = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLSpanElement>(null);
  const statusSize = useRef<HTMLSpanElement>(null);
  const foldedFocus = useRef(false);
  const [fit, setFit] = useState({ shortcuts: false, status: false });
  const platform = currentPlatform();
  useLayoutEffect(() => {
    const element = bar.current;
    if (!element) return;
    const measure = () => {
      const available = element.clientWidth;
      const preferred = statusSize.current?.getBoundingClientRect().width ?? 0;
      if (!(available > 0 && preferred > 0)) return;
      const width = (node: Element | null) => node?.getBoundingClientRect().width ?? 0;
      const gap = parseFloat(getComputedStyle(element).columnGap) || 0;
      const hintStyle = hintsRow.current ? getComputedStyle(hintsRow.current) : null;
      const innerGap = parseFloat(hintStyle?.columnGap ?? "") || 0;
      const hintMargin = (parseFloat(hintStyle?.marginInlineStart ?? "") || 0) + (parseFloat(hintStyle?.marginInlineEnd ?? "") || 0);
      const actions = [...(hintsRow.current?.children ?? [])].filter((child) => child !== shortcut.current);
      const optional = width(shortcut.current);
      const actionWidth = actions.reduce((sum, child) => sum + width(child), 0);
      const pinWidth = width(pin.current), menuWidth = width(menu.current?.querySelector("button") ?? null);
      const needed = (showShortcut: boolean) => {
        const count = actions.length + (showShortcut && optional > 0 ? 1 : 0);
        const bands = 1 + (pinWidth > 0 ? 1 : 0) + (menuWidth > 0 ? 1 : 0) + (count > 0 ? 1 : 0);
        return pinWidth + menuWidth + actionWidth + (showShortcut ? optional : 0) + Math.max(0, count - 1) * innerGap + (bands - 1) * gap + (count > 0 ? hintMargin : 0);
      };
      const shortcuts = preferred + needed(true) > available;
      const status = preferred + needed(!shortcuts) > available;
      if (shortcuts && shortcut.current?.contains(document.activeElement)) foldedFocus.current = true;
      setFit((previous) => previous.shortcuts === shortcuts && previous.status === status ? previous : { shortcuts, status });
    };
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    for (const node of [element, statusSize.current, pin.current, ...(hintsRow.current?.children ?? []), menu.current]) if (node) observer?.observe(node);
    window.addEventListener("resize", measure);
    return () => { observer?.disconnect(); window.removeEventListener("resize", measure); };
  }, [hints.enter, hints.shiftEnter, hints.escape, altHeld, pinned, platform]);
  useLayoutEffect(() => {
    if (!foldedFocus.current || !fit.shortcuts) return;
    foldedFocus.current = false;
    if (document.activeElement === document.body || shortcut.current?.contains(document.activeElement)) menu.current?.querySelector("button")?.focus({ preventScroll: true });
  }, [fit.shortcuts]);
  return (
    <footer>
      <section ref={bar} aria-label={pickerText.libraryRegion} className="library-bar" data-shortcuts-folded={fit.shortcuts || undefined} data-status-compact={fit.status || undefined}>
        <span className="footer-fit-measure" aria-hidden="true" inert><span ref={statusSize} className="footer-status-size" /></span>
        <LibraryStatusLine library={library} groupName={groupName} />
        {pinned && <Button ref={pin} variant="icon" className="pin-button" aria-label={pickerText.unpinTitle} title={pickerText.unpinTitle} onPress={onUnpin}>
          <svg aria-hidden="true" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor">
            <path d="M9 4h6l-1 5 4 4H6l4-4z" /><path d="M12 13v7" />
          </svg>
        </Button>}
        <small ref={hintsRow} data-optional-only={!altHeld && !hints.enter || undefined} className="enter-hints" aria-label={keyText.footer} role="group">
          {altHeld ? <>
            {hints.shiftEnter && <FooterAction label={hints.shiftEnter} keys={keysFor("add")} onAction={actions.add} />}
            <FooterAction label={hints.escape} keys={keysFor("escape")} onAction={actions.escape} />
          </> : <>
            {hints.enter && <FooterAction label={hints.enter} keys={keysFor("copy")} primary onAction={actions.copy} />}
            <FooterAction label={keyText.allKeys} keys={keysFor("keys")} className="shortcuts-hint" folded={fit.shortcuts} controlRef={shortcut} onAction={actions.keys} />
          </>}
        </small>
        <span ref={menu} className="more">
          {more}
        </span>
        {library?.note && <p role="status">{library.note}</p>}
      </section>
    </footer>
  );
}

function FooterAction({ label, keys, primary, className = "", folded, controlRef, onAction }: {
  label: string; keys: string; primary?: boolean; className?: string; folded?: boolean; controlRef?: React.Ref<HTMLButtonElement>; onAction: () => void;
}) {
  const platform = currentPlatform();
  return <Button ref={controlRef} variant="quiet" size="small" className={`footer-action ${className}`} aria-label={label} aria-hidden={folded || undefined} inert={folded || undefined} excludeFromTabOrder={folded}
    aria-keyshortcuts={ariaKeys(keys, platform === "macos")} onPress={onAction}>
    <KeyHint label={label} keys={keys} primary={primary} platform={platform} hidden />
  </Button>;
}
