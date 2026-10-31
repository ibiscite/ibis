import { Children, createContext, isValidElement, useCallback, useContext, useEffect, useEffectEvent, useLayoutEffect, useRef, useState, type ComponentPropsWithRef, type PointerEvent, type ReactElement, type ReactNode, type Ref, type RefObject } from "react";
import { composing as isComposing } from "../composition";
import { flushSync } from "react-dom";
import { mergeProps, setInteractionModality } from "react-aria";
import { Header, Menu as AriaMenu, MenuItem as AriaMenuItem, MenuSection as AriaMenuSection, MenuStateContext, MenuTrigger, Popover, Separator, SubmenuTrigger, Text } from "react-aria-components";
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
export function useMenuBoundary(open: boolean, returnRef?: RefObject<HTMLElement | null>, trigger?: RefObject<HTMLElement | null>, options?: {
  overlayRef: RefObject<HTMLElement | null>; scrollRef: RefObject<HTMLElement | null>; onAnchorUnavailable: () => void;
}) {
  const [boundary, setBoundary] = useState<HTMLElement>();
  const [clearBottom, setClearBottom] = useState(0);
  const { overlayRef, scrollRef } = options ?? {};
  const unavailable = useEffectEvent(() => options?.onAnchorUnavailable());
  useLayoutEffect(() => {
    if (!open) return;
    let current = true, frame = 0, signature = "";
    let node: HTMLElement | null = null;
    const floor = document.querySelector<HTMLElement>("[data-menu-floor]");
    const above = floor && trigger?.current && !floor.contains(trigger.current) ? floor : null;
    const layout = trigger?.current?.closest("main") ?? trigger?.current?.parentElement;
    const update = (validate = false) => {
      if (!current) return;
      const bottom = Math.max(0, returnRef?.current?.getBoundingClientRect().bottom ?? 0);
      // Never above the top edge, even in a window too short for both.
      const end = above && window.innerHeight > 400 ? Math.max(bottom, above.getBoundingClientRect().top) : window.innerHeight;
      const anchor = trigger?.current;
      const rect = anchor?.getBoundingClientRect();
      if (validate && overlayRef && anchor && (!anchor.isConnected || anchor.closest("[hidden]"))) { unavailable(); return; }
      if (validate && overlayRef && scrollRef && anchor && rect && rect.width > 0 && rect.height > 0) {
        // A details menu must retain a visible cause. Do not invent an anchor
        // at the window edge after its segment leaves a scrolling viewport.
        let top = Math.max(0, rect.top), left = Math.max(0, rect.left);
        let right = Math.min(innerWidth, rect.right), endVisible = Math.min(innerHeight, rect.bottom);
        for (let parent = anchor.parentElement; parent; parent = parent.parentElement) {
          const style = getComputedStyle(parent), bounds = parent.getBoundingClientRect();
          if (/auto|scroll|hidden|clip/.test(style.overflowY) && bounds.height > 0) { top = Math.max(top, bounds.top); endVisible = Math.min(endVisible, bounds.bottom); }
          if (/auto|scroll|hidden|clip/.test(style.overflowX) && bounds.width > 0) { left = Math.max(left, bounds.left); right = Math.min(right, bounds.right); }
        }
        const overlay = overlayRef.current, scroll = scrollRef.current;
        const box = overlay?.getBoundingClientRect(), port = scroll?.getBoundingClientRect();
        // Keep the fixed heading and at least one whole focus target usable;
        // everything else can scroll in the menu. Initial zero geometry waits
        // for layout rather than dismissing before the first measurement.
        let minimum = 0;
        if (overlay && scroll && box && port && box.height > 0) {
          const style = getComputedStyle(overlay);
          const control = scroll.querySelector<HTMLElement>('input, button, [tabindex="0"]') ?? overlay.querySelector<HTMLElement>("button");
          const row = control?.getBoundingClientRect().height ?? 0;
          minimum = Math.max(0, port.top - box.top) + (parseFloat(style.paddingBottom) || 0) + (parseFloat(style.borderBottomWidth) || 0) + Math.min(scroll.scrollHeight, row);
        }
        const space = Math.max(rect.top - bottom, end - rect.bottom) - 8 - 6;
        if (right <= left || endVisible <= top || space <= 0 || space < minimum) { unavailable(); return; }
      }
      const next = [innerWidth, innerHeight, bottom, end, rect?.x, rect?.y, rect?.width, rect?.height].join(",");
      if (next === signature) return;
      signature = next;
      const previous = node;
      node = document.createElement("div");
      node.dataset.menuBoundary = "";
      node.setAttribute("aria-hidden", "true");
      Object.assign(node.style, { position: "fixed", inset: "0", top: `${bottom}px`, bottom: `${Math.max(0, innerHeight - end)}px`, pointerEvents: "none" });
      document.body.append(node);
      // React Aria observes target size, not position-only layout changes.
      // A fresh boundary asks that same fitting owner to recompute placement
      // and its height cap, without remounting the focused menu content.
      setBoundary(node);
      setClearBottom(bottom);
      previous?.remove();
    };
    update();
    const schedule = () => {
      if (!current || frame) return;
      frame = requestAnimationFrame(() => { frame = 0; update(true); });
    };
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(schedule);
    if (returnRef?.current) observer?.observe(returnRef.current);
    if (above) observer?.observe(above);
    if (trigger?.current) observer?.observe(trigger.current);
    if (overlayRef?.current) observer?.observe(overlayRef.current);
    if (scrollRef?.current) observer?.observe(scrollRef.current);
    // A neighbour can move a fixed-size anchor: header wrapping, middle-area
    // settling, shelf folding and a footer growing all belong to this owner.
    if (layout) {
      observer?.observe(layout);
      for (const child of layout.children) observer?.observe(child);
    }
    const changed = new MutationObserver(schedule);
    if (layout) changed.observe(layout, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ["class", "style", "hidden", "data-folded"] });
    window.addEventListener("resize", schedule);
    window.visualViewport?.addEventListener("resize", schedule);
    document.addEventListener("scroll", schedule, true);
    schedule();
    return () => {
      current = false;
      cancelAnimationFrame(frame);
      observer?.disconnect();
      changed.disconnect();
      window.removeEventListener("resize", schedule);
      window.visualViewport?.removeEventListener("resize", schedule);
      document.removeEventListener("scroll", schedule, true);
      node?.remove();
    };
  }, [open, returnRef, trigger, overlayRef, scrollRef]);
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
          // Rectangles already include the scroll offset. Fit the last whole
          // row in this viewport without moving the user's scroll position.
          const bottom = row.getBoundingClientRect().bottom - top;
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

const ActionFocusContext = createContext<((immediate?: boolean) => void) | null>(null);

/** A wrapping, read-only menu header, also spoken through its section label. */
export function MenuDetail({ title, detail }: { title: string; detail?: string | null }) {
  return <p className="menu-detail" aria-hidden="true">
    <span className="menu-detail-title">{title}</span>
    {detail && <span>{detail}</span>}
  </p>;
}

type ItemProps = {
  id: string; label: string; title?: string; keys?: string; icon?: ReactNode; example?: string;
  /** A toggle: shows a check and is a menuitemcheckbox. */
  checked?: boolean;
  /** Stays focusable and is announced as unavailable, but does nothing (ADR 0021). */
  disabled?: boolean;
  closeOnSelect?: boolean;
  onAction: () => void;
};

/** A drawn check, so it adds no text to the row; Menu.css places it per platform. */
export function MenuCheck({ checked }: { checked: boolean }) {
  return <span className="menu-check" aria-hidden="true">
    {checked && <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><path d="m2.5 6.5 2.5 2.5 4.5-6" /></svg>}
  </span>;
}

/** An action row: label, optional shortcut and a check for toggles. */
export function MenuItem({ id, label, title, keys, checked, disabled, closeOnSelect, onAction, icon, example }: ItemProps) {
  const requestActionFocus = useContext(ActionFocusContext);
  return <AriaMenuItem id={id} className="menu-item" textValue={label} aria-label={example ? label : undefined}
    // A toggle keeps the menu open, as before, so several can be changed at once.
    shouldCloseOnSelect={closeOnSelect ?? (checked === undefined && !disabled ? undefined : false)}
    onAction={() => {
      if (disabled) return;
      if (checked === undefined || closeOnSelect) requestActionFocus?.(closeOnSelect);
      onAction();
    }}
    render={(props) => <MenuRow {...props as ComponentPropsWithRef<"div">} itemId={id} title={title} aria-disabled={disabled || undefined} />}>
    {({ isSelected }) => <>
      {checked !== undefined && <MenuCheck checked={isSelected} />}
      {icon}
      <bdi className="menu-label" dir="auto">{label}</bdi>
      {example && <Text slot="description" className="menu-example"><bdi dir="auto">{example}</bdi></Text>}
      {keys && <span className="menu-keys" aria-hidden="true"><bdi dir="ltr">{keys}</bdi></span>}
    </>}
  </AriaMenuItem>;
}

/**
 * A group of items. Items with `checked` make it a checkbox group, whose
 * checked keys come from the items themselves. `header` is a visual line
 * above the items that names the group when no `label` is given.
 */
export function MenuSection({ id, label, header, children, layout, selectionMode = "multiple" }: { id: string; label?: string; header?: ReactNode; children: ReactNode; layout?: "chips"; selectionMode?: "single" | "multiple" }) {
  const items = Children.toArray(children).filter(isValidElement) as ReactElement<ItemProps>[];
  const checkable = items.some((item) => item.props.checked !== undefined);
  return <AriaMenuSection id={id} aria-label={label} className={`menu-section${layout === "chips" ? " menu-section-chips" : ""}`}
    selectionMode={checkable ? selectionMode : undefined}
    selectedKeys={checkable ? items.filter((item) => item.props.checked).map((item) => item.props.id) : undefined}>
    {header && <Header className="menu-header">{label && <span className="visually-hidden">{label}</span>}<div aria-hidden="true">{header}</div></Header>}
    {children}
  </AriaMenuSection>;
}

/** A nested choice menu. React Aria owns hover, arrows, Escape and placement;
 * the parent menu still owns Tab and action focus return. */
export function MenuSubmenu({ id, label, value, title, children }: {
  id: string; label: string; value: string; title?: string; children: ReactNode;
}) {
  const list = useRef<HTMLDivElement | null>(null);
  const wholeRows = useWholeRows(list);
  return <SubmenuTrigger>
    <AriaMenuItem id={id} className="menu-item" textValue={value} aria-label={value}
      render={(props) => <MenuRow {...props as ComponentPropsWithRef<"div">} itemId={id} title={title} />}>
      <bdi className="menu-label" dir="auto">{value}</bdi>
      <span className="menu-submenu-arrow" aria-hidden="true">›</span>
    </AriaMenuItem>
    <Popover ref={wholeRows} className="menu-popover ui-overlay" offset={6} containerPadding={8}>
      <AriaMenu ref={list} className="menu scroll-area" aria-label={label}
        render={(props) => <div {...props as ComponentPropsWithRef<"div">} aria-label={label} aria-labelledby={undefined} />}>
        {children}
      </AriaMenu>
    </Popover>
  </SubmenuTrigger>;
}

export function MenuSeparator() {
  return <Separator className="menu-separator" />;
}

type MenuProps = {
  /** The menu's accessible name. */
  label: string; id?: string;
  /** The trigger button's content and attributes. */
  trigger: ReactNode; triggerRef?: Ref<HTMLButtonElement>; triggerClassName?: string; triggerTitle?: string; triggerKeys?: string; triggerLabel?: string;
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
export function Menu({ label, id, trigger, triggerRef, triggerClassName, triggerTitle, triggerKeys, triggerLabel, isOpen, onOpenChange, tabFocusRef, placement = "bottom end", children }: MenuProps) {
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
  // Outside the collection builder so submenu portals inherit the action owner.
  return <ActionFocusContext.Provider value={(immediate) => {
    // Release both modal scopes before returning focus from a submenu choice.
    if (immediate) leave(tabFocusRef?.current, true);
    else pendingFocus.current = tabFocusRef?.current ?? null;
  }}><MenuTrigger isOpen={isOpen} onOpenChange={(open) => { fromTrigger.current = open; onOpenChange(open); }}>
    <Button variant="quiet" size="small" aria-label={label} ref={(node) => {
      button.current = node;
      if (typeof triggerRef === "function") triggerRef(node);
      else if (triggerRef) triggerRef.current = node;
    }} className={triggerClassName}
      render={(props) => <button {...props as ComponentPropsWithRef<"button">} title={triggerTitle} aria-label={triggerLabel ?? label} aria-keyshortcuts={triggerKeys} />}>{trigger}</Button>
    <Popover ref={popoverRef} className="menu-popover ui-overlay" placement={renderedPlacement} offset={6} containerPadding={8} shouldSkipAnimation={skipExit} {...boundary}>
      <div className="menu-frame" onKeyDownCapture={(event) => {
        if (isComposing(event)) { event.stopPropagation(); return; }
        // Escape leaves the innermost submenu first. Tab leaves the entire menu.
        if (event.key === "Escape" && event.target instanceof Element && event.target.closest('[role="menu"]') !== list.current) return;
        const to = event.key === "Escape" ? button.current : event.key === "Tab" ? tabFocusRef?.current : undefined;
        if (to === undefined) return;
        event.preventDefault();
        event.stopPropagation();
        leave(to, event.key === "Tab");
      }} onKeyUpCapture={(event) => { if (isComposing(event)) event.stopPropagation(); }}>
        <AriaMenu key={opening} ref={listRef} id={id} className="menu scroll-area" aria-label={label} autoFocus={fromTrigger.current ? undefined : "first"}
          render={(props) => <div {...props as ComponentPropsWithRef<"div">} aria-label={label} aria-labelledby={undefined} />}>
          {children}
        </AriaMenu>
      </div>
    </Popover>
  </MenuTrigger></ActionFocusContext.Provider>;
}
