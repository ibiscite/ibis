import { Children, createContext, isValidElement, useCallback, useContext, useEffect, useLayoutEffect, useRef, useState, type ComponentPropsWithRef, type PointerEvent, type ReactElement, type ReactNode, type Ref, type RefObject } from "react";
import { composing as isComposing } from "../composition";
import { flushSync } from "react-dom";
import { mergeProps, setInteractionModality } from "react-aria";
import { Header, Menu as AriaMenu, MenuItem as AriaMenuItem, MenuSection as AriaMenuSection, MenuStateContext, MenuTrigger, Popover, Separator } from "react-aria-components";
import { Button } from "./Button";
import { markScrollEdges } from "./ScrollArea";
import { useMenuLifecycle } from "./useMenuLifecycle";
import "./Menu.css";
import "./Overlay.css";

/** Align to the rendered trailing edge even when the browser locale differs. */
export function useMenuPlacement(open: boolean, trigger: RefObject<HTMLElement | null>, placement: "top end" | "bottom end") {
  const [rtl, setRtl] = useState(false);
  useLayoutEffect(() => {
    if (open && trigger.current) setRtl(getComputedStyle(trigger.current).direction === "rtl");
  }, [open, trigger]);
  return `${placement === "top end" ? "top" : "bottom"} ${rtl ? "left" : "right"}` as const;
}

/** Keep React Aria's placement boundary inside the viewport and below the
 * return field. A wrapped picker can put its footer trigger below the window.
 * A menu opened from above the `[data-menu-floor]` region (the footer) also
 * ends above it, so it never covers the footer's actions. */
export function useMenuBoundary(open: boolean, returnRef?: RefObject<HTMLElement | null>, trigger?: RefObject<HTMLElement | null>) {
  const [boundary, setBoundary] = useState<HTMLElement>();
  const [clearBottom, setClearBottom] = useState(0);
  useLayoutEffect(() => {
    if (!open) return;
    const node = document.createElement("div");
    node.dataset.menuBoundary = "";
    node.setAttribute("aria-hidden", "true");
    Object.assign(node.style, { position: "fixed", inset: "0", pointerEvents: "none" });
    const floor = document.querySelector<HTMLElement>("[data-menu-floor]");
    const above = floor && trigger?.current && !floor.contains(trigger.current) ? floor : null;
    const update = () => {
      const bottom = Math.max(0, returnRef?.current?.getBoundingClientRect().bottom ?? 0);
      node.style.top = `${bottom}px`;
      // Never above the top edge, even in a window too short for both.
      if (above) node.style.bottom = window.innerHeight <= 400 ? "0px" : `${Math.max(0, window.innerHeight - Math.max(bottom, above.getBoundingClientRect().top))}px`;
      setClearBottom(bottom);
    };
    update();
    document.body.append(node);
    setBoundary(node);
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(update);
    if (returnRef?.current) observer?.observe(returnRef.current);
    if (above) observer?.observe(above);
    window.addEventListener("resize", update);
    window.visualViewport?.addEventListener("resize", update);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", update);
      window.visualViewport?.removeEventListener("resize", update);
      node.remove();
    };
  }, [open, returnRef, trigger]);
  const getTargetRect = useCallback((target: Element) => {
    const rect = target.getBoundingClientRect();
    if (!boundary?.isConnected) return rect;
    const bounds = boundary.getBoundingClientRect();
    // Preserve the real anchor when visible; shift an off-window trigger into
    // the boundary so placement can still flip and cap against usable space.
    if (rect.top >= 8 && rect.bottom <= bounds.bottom - 8) return rect;
    const top = Math.max(clearBottom + 8 - 6 - rect.height, Math.min(rect.top, bounds.bottom - 8));
    return new DOMRect(rect.x, top, rect.width, rect.height);
  }, [boundary, clearBottom]);
  return { boundaryElement: boundary, getTargetRect };
}

/** A scrolling menu ends on a whole row, never a cut one (#1031). Pass the
 * returned callback the popover node; it refits whenever React Aria resizes
 * the popover or the rows change, as when filtering. */
export function useWholeRows(list: RefObject<HTMLElement | null>) {
  const stop = useRef<(() => void) | null>(null);
  return useCallback((frame: HTMLElement | null) => {
    stop.current?.();
    stop.current = null;
    if (!frame) return;
    let current = true;
    const ownedList = () => current && list.current && frame.contains(list.current) ? list.current : null;
    // React Aria owns scrolling; fade updates never move the user's viewport.
    const edges = () => { const node = ownedList(); if (node) markScrollEdges(node); };
    const fit = () => {
      const node = ownedList();
      if (!node) return;
      node.style.maxHeight = "";
      const height = node.clientHeight;
      if (node.scrollHeight > height) {
        const top = node.getBoundingClientRect().top + node.clientTop;
        let end = 0;
        for (const row of node.querySelectorAll('[role^="menuitem"]')) {
          const bottom = row.getBoundingClientRect().bottom - top + node.scrollTop;
          if (bottom <= height && bottom > end) end = bottom;
        }
        if (end > 0) node.style.maxHeight = `${Math.ceil(end)}px`;
      }
      for (const row of node.querySelectorAll('[role^="menuitem"]')) geometry?.observe(row);
      edges();
    };
    const geometry = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(fit);
    geometry?.observe(frame);
    if (list.current) geometry?.observe(list.current);
    // The frame's own style (React Aria's max-height) and the rows, never the
    // list's style or edges, which fit() itself sets.
    const resized = new MutationObserver(fit);
    const rows = new MutationObserver((records) => { if (records.some((record) => record.type !== "attributes")) fit(); else edges(); });
    resized.observe(frame, { attributes: true, attributeFilter: ["style"] });
    rows.observe(frame, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ["data-focused"] });
    // Scrolling does not bubble; the capture sees the list's, also once it is remounted.
    frame.addEventListener("scroll", edges, true);
    fit();
    stop.current = () => { current = false; geometry?.disconnect(); resized.disconnect(); rows.disconnect(); frame.removeEventListener("scroll", edges, true); };
  }, [list]);
}

let lastPointer = "";

/** A menu item's row. Any pointer movement over it highlights it, as in
 * native menus: React Aria moves the highlight only when the pointer enters an
 * item, and not at all while arrow keys are in use (#1031). Rendered by the
 * item, so it reads the menu's own state. */
export function MenuRow({ itemId, ...props }: ComponentPropsWithRef<"div"> & { itemId: string }) {
  const state = useContext(MenuStateContext);
  const onPointerMove = (event: PointerEvent) => {
    // A pointer at rest gets a synthetic move when arrow keys scroll rows
    // under it; only a real movement takes the highlight.
    const at = `${event.clientX},${event.clientY}`;
    if (at === lastPointer) return;
    lastPointer = at;
    const manager = state?.selectionManager;
    if (event.pointerType === "touch" || !manager || (manager.isFocused && manager.focusedKey === itemId)) return;
    setInteractionModality("pointer");
    manager.setFocused(true);
    manager.setFocusedKey(itemId);
  };
  return <div {...mergeProps(props, { onPointerMove })} />;
}

const ActionFocusContext = createContext<(() => void) | null>(null);

/** A wrapping, read-only menu header, also spoken through its section label. */
export function MenuDetail({ title, detail }: { title: string; detail?: string | null }) {
  return <p className="menu-detail" aria-hidden="true">
    <span className="menu-detail-title">{title}</span>
    {detail && <span>{detail}</span>}
  </p>;
}

type ItemProps = {
  id: string; label: string; title?: string; keys?: string;
  /** A toggle: shows a check and is a menuitemcheckbox. */
  checked?: boolean;
  /** Stays focusable and is announced as unavailable, but does nothing (ADR 0021). */
  disabled?: boolean;
  onAction: () => void;
};

/** A drawn check, so it adds no text to the row; Menu.css places it per platform. */
export function MenuCheck({ checked }: { checked: boolean }) {
  return <span className="menu-check" aria-hidden="true">
    {checked && <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><path d="m2.5 6.5 2.5 2.5 4.5-6" /></svg>}
  </span>;
}

/** An action row: label, optional shortcut and a check for toggles. */
export function MenuItem({ id, label, title, keys, checked, disabled, onAction }: ItemProps) {
  const requestActionFocus = useContext(ActionFocusContext);
  return <AriaMenuItem id={id} className="menu-item" textValue={label}
    // A toggle keeps the menu open, as before, so several can be changed at once.
    shouldCloseOnSelect={checked === undefined && !disabled ? undefined : false}
    onAction={() => {
      if (disabled) return;
      if (checked === undefined) requestActionFocus?.();
      onAction();
    }}
    render={(props) => <MenuRow {...props as ComponentPropsWithRef<"div">} itemId={id} title={title} aria-disabled={disabled || undefined} />}>
    {({ isSelected }) => <>
      {checked !== undefined && <MenuCheck checked={isSelected} />}
      <span className="menu-label">{label}</span>
      {keys && <span className="menu-keys" aria-hidden="true"><bdi dir="ltr">{keys}</bdi></span>}
    </>}
  </AriaMenuItem>;
}

/**
 * A group of items. Items with `checked` make it a checkbox group, whose
 * checked keys come from the items themselves. `header` is a visual line
 * above the items that names the group when no `label` is given.
 */
export function MenuSection({ id, label, header, children }: { id: string; label?: string; header?: ReactNode; children: ReactNode }) {
  const items = Children.toArray(children).filter(isValidElement) as ReactElement<ItemProps>[];
  const checkable = items.some((item) => item.props.checked !== undefined);
  return <AriaMenuSection id={id} aria-label={label} className="menu-section"
    selectionMode={checkable ? "multiple" : undefined}
    selectedKeys={checkable ? items.filter((item) => item.props.checked).map((item) => item.props.id) : undefined}>
    {header && <Header className="menu-header">{label && <span className="visually-hidden">{label}</span>}<div aria-hidden="true">{header}</div></Header>}
    {children}
  </AriaMenuSection>;
}

export function MenuSeparator() {
  return <Separator className="menu-separator" />;
}

type MenuProps = {
  /** The menu's accessible name. */
  label: string; id?: string;
  /** The trigger button's content and attributes. */
  trigger: ReactNode; triggerRef?: Ref<HTMLButtonElement>; triggerClassName?: string; triggerTitle?: string; triggerKeys?: string;
  isOpen: boolean; onOpenChange: (open: boolean) => void;
  /** Where Tab goes: leaving the menu returns there so keys keep working. */
  tabFocusRef?: RefObject<HTMLElement | null>;
  placement?: "top end" | "bottom end";
  children: ReactNode;
};

/**
 * A menu button with a popover menu. React Aria owns roles, arrow keys,
 * typeahead, Escape, outside clicks, focus return and placement. Opened by a
 * pointer, nothing is highlighted until the pointer or an arrow key moves;
 * opened from the keyboard, including a shortcut that sets `isOpen`, the first
 * item is highlighted, as in native menus.
 */
export function Menu({ label, id, trigger, triggerRef, triggerClassName, triggerTitle, triggerKeys, isOpen, onOpenChange, tabFocusRef, placement = "bottom end", children }: MenuProps) {
  const fromTrigger = useRef(false);
  const button = useRef<HTMLButtonElement | null>(null);
  const boundary = useMenuBoundary(isOpen, tabFocusRef, button);
  const renderedPlacement = useMenuPlacement(isOpen, button, placement);
  const { list, listRef, opening, retryFocus, cancelFocusRetry } = useMenuLifecycle(isOpen);
  const popover = useRef<HTMLElement | null>(null);
  const wholeRows = useWholeRows(list);
  const pendingFocus = useRef<HTMLElement | null>(null);
  const [skipExit, setSkipExit] = useState(false);
  useEffect(() => { if (!isOpen) fromTrigger.current = false; }, [isOpen]);
  useEffect(() => {
    if (!isOpen) return;
    // A smaller window shrinks the menu; keep the highlighted row in view.
    const keepFocusedVisible = () => {
      const node = list.current;
      const active = document.activeElement;
      if (!node || !(active instanceof HTMLElement) || !node.contains(active)) return;
      const top = node.getBoundingClientRect().top + node.clientTop;
      const bottom = top + node.clientHeight;
      const item = active.getBoundingClientRect();
      if (item.top < top) node.scrollTop += Math.floor(item.top - top);
      else if (item.bottom > bottom) node.scrollTop += Math.ceil(item.bottom - bottom);
    };
    window.addEventListener("resize", keepFocusedVisible);
    return () => window.removeEventListener("resize", keepFocusedVisible);
  }, [isOpen, list]);
  // The exiting modal popover can refuse focus outside it. Retry after removal,
  // and leave a newly focused dialog/control alone.
  const restoreFocus = useCallback(() => {
    const target = pendingFocus.current;
    const active = document.activeElement;
    if (target?.isConnected && (active === document.body || active === button.current || active === target || popover.current?.contains(active))) target.focus();
    if (!popover.current) pendingFocus.current = null;
  }, []);
  const popoverRef = useCallback((node: HTMLElement | null) => {
    popover.current = node;
    wholeRows(node);
    if (!node && pendingFocus.current) retryFocus(restoreFocus);
  }, [restoreFocus, retryFocus, wholeRows]);
  useLayoutEffect(() => {
    if (isOpen) {
      setSkipExit(false);
      pendingFocus.current = null;
    } else restoreFocus();
  }, [isOpen, restoreFocus]);
  useEffect(() => {
    // Any later user input supersedes the closing action's destination.
    const cancel = () => { pendingFocus.current = null; };
    document.addEventListener("pointerdown", cancel, true);
    document.addEventListener("keydown", cancel, true);
    return () => {
      cancel();
      document.removeEventListener("pointerdown", cancel, true);
      document.removeEventListener("keydown", cancel, true);
    };
  }, []);
  const leave = (to: HTMLElement | null | undefined, immediate: boolean) => {
    if (immediate) {
      // Tab hands input back now. Keeping the modal mounted for its exit would
      // reject focus, losing any typing before its animation finishes.
      const previous = document.activeElement;
      pendingFocus.current = null;
      cancelFocusRetry();
      flushSync(() => { setSkipExit(true); onOpenChange(false); });
      const active = document.activeElement;
      if (to?.isConnected && (active === previous || active === document.body || active === button.current || active === to)) to.focus();
      return;
    }
    pendingFocus.current = to ?? null;
    onOpenChange(false);
  };
  return <MenuTrigger isOpen={isOpen} onOpenChange={(open) => { fromTrigger.current = open; onOpenChange(open); }}>
    <Button variant="quiet" size="small" ref={(node) => {
      button.current = node;
      if (typeof triggerRef === "function") triggerRef(node);
      else if (triggerRef) triggerRef.current = node;
    }} className={triggerClassName}
      render={(props) => <button {...props as ComponentPropsWithRef<"button">} title={triggerTitle} aria-keyshortcuts={triggerKeys} />}>{trigger}</Button>
    <Popover ref={popoverRef} className="menu-popover ui-overlay" placement={renderedPlacement} offset={6} containerPadding={8} shouldSkipAnimation={skipExit} {...boundary}>
      <div className="menu-frame" onKeyDownCapture={(event) => {
        if (isComposing(event)) { event.stopPropagation(); return; }
        const to = event.key === "Escape" ? button.current : event.key === "Tab" ? tabFocusRef?.current : undefined;
        if (to === undefined) return;
        event.preventDefault();
        event.stopPropagation();
        leave(to, event.key === "Tab");
      }} onKeyUpCapture={(event) => { if (isComposing(event)) event.stopPropagation(); }}>
        <AriaMenu key={opening} ref={listRef} id={id} className="menu scroll-area" aria-label={label} autoFocus={fromTrigger.current ? undefined : "first"}>
          <ActionFocusContext.Provider value={() => { pendingFocus.current = tabFocusRef?.current ?? null; }}>
            {children}
          </ActionFocusContext.Provider>
        </AriaMenu>
      </div>
    </Popover>
  </MenuTrigger>;
}
