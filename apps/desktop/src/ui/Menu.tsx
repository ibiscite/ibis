import { Icon } from "../icons";
import { Children, createContext, isValidElement, useCallback, useContext, useEffect, useId, useLayoutEffect, useRef, useState, type ComponentPropsWithRef, type PointerEvent, type ReactElement, type ReactNode, type Ref, type RefObject } from "react";
import { composing as isComposing } from "../composition";
import { flushSync } from "react-dom";
import { mergeProps, setInteractionModality } from "react-aria";
import { Header, Menu as AriaMenu, MenuItem as AriaMenuItem, MenuSection as AriaMenuSection, MenuStateContext, MenuTrigger, Separator, Text } from "react-aria-components";
import { Button } from "./Button";
import { markScrollEdges } from "./ScrollArea";
import { Floating, useViewFallback } from "./Layer";
import { useLayer, type LayerClose } from "./layers";
import { keepKeyboardThroughPress, ViewFrame } from "./ViewFrame";
import { say } from "../i18n";
import { LEAST_ROWS } from "./region";
import { useMenuLifecycle } from "./useMenuLifecycle";
import { usePresence, usePresenceSlot } from "./presence";
import "./Menu.css";
import "./Overlay.css";

/** Align to the rendered trailing edge even when the browser locale differs. */
export function useMenuPlacement(open: boolean, trigger: RefObject<HTMLElement | null>, placement: "top start" | "top end" | "bottom end" | "bottom start") {
  const [rtl, setRtl] = useState(false);
  useLayoutEffect(() => {
    if (open && trigger.current) setRtl(getComputedStyle(trigger.current).direction === "rtl");
  }, [open, trigger]);
  const end = placement.endsWith("end") !== rtl;
  return `${placement.startsWith("top") ? "top" : "bottom"} ${end ? "right" : "left"}` as const;
}

/** What an anchored menu must show, measured on its drawn surface: its chrome
 * (padding, heading, note, filter) and its first `LEAST_ROWS` whole rows,
 * or all of them when it has fewer; with no rows, its empty state. Read from
 * the content's own sizes, so a capped, scrolling menu measures the same. */
export function menuLeast(surface: HTMLElement): number {
  const list = surface.querySelector<HTMLElement>('[role="menu"]');
  if (!list) return surface.scrollHeight;
  const chrome = surface.scrollHeight - list.offsetHeight;
  const rows = list.querySelectorAll('[role^="menuitem"]');
  const last = rows[Math.min(LEAST_ROWS, rows.length) - 1];
  // Rows' boxes already include the list's scroll offset.
  const top = list.getBoundingClientRect().top - list.scrollTop;
  return Math.ceil(chrome + (last ? last.getBoundingClientRect().bottom - top : list.scrollHeight));
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
  // Named, so a menu that turns into a view opens on the row it had highlighted.
  return <div {...mergeProps(props, { onPointerMove })} data-item={itemId} />;
}

const ActionFocusContext = createContext<(() => void) | null>(null);
const Columns = createContext<((slot: "icons", on: boolean) => void) | undefined>(undefined);

function MenuList(props: React.ComponentProps<typeof AriaMenu>) {
  const { counts, register } = usePresence({ icons: 0 });
  return <Columns.Provider value={register}><AriaMenu {...props} data-icons={counts.icons ? "" : undefined} /></Columns.Provider>;
}

/** A wrapping, read-only menu header, also spoken through its section label.
 * Each fact of the detail ("My Library · checked 3 minutes ago, 1:54 AM") takes
 * its own line, so a menu's width never breaks a phrase such as a time apart. */
export function MenuDetail({ title, detail }: { title: string; detail?: string | null }) {
  return <p className="menu-detail" aria-hidden="true">
    <span className="menu-detail-title">{title}</span>
    {detail?.split(" · ").map((fact) => <span key={fact}>{fact}</span>)}
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

/** A drawn check, so it adds no text to the row; Menu.css gives it the icon column. */
export function MenuCheck({ checked, icon }: { checked: boolean; icon?: ReactNode }) {
  usePresenceSlot(useContext(Columns), "icons");
  return <span className="menu-check" aria-hidden="true">
    {checked ? <Icon name="check" /> : icon}
  </span>;
}

/** An action row: label, optional shortcut and a check for toggles. */
export function MenuItem({ id, label, title, keys, checked, disabled, closeOnSelect, onAction, icon, example }: ItemProps) {
  const requestActionFocus = useContext(ActionFocusContext);
  return <AriaMenuItem id={id} className="menu-item" data-icon={checked !== undefined || icon ? "" : undefined} textValue={label} aria-label={example ? label : undefined}
    // A toggle keeps the menu open, as before, so several can be changed at once.
    shouldCloseOnSelect={closeOnSelect ?? (checked === undefined && !disabled ? undefined : false)}
    onAction={() => {
      if (disabled) return;
      if (checked === undefined || closeOnSelect) requestActionFocus?.();
      onAction();
    }}
    render={(props) => <MenuRow {...props as ComponentPropsWithRef<"div">} itemId={id} title={title} aria-disabled={disabled || undefined} />}>
    {({ isSelected }) => <>
      {checked !== undefined ? <MenuCheck checked={isSelected} /> : icon && <MenuCheck checked={false} icon={icon} />}
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
export function MenuSection({ id, label, header, children, layout, selectionMode = "multiple" }: { id: string; label?: string; header?: ReactNode; children: ReactNode; layout?: "list"; selectionMode?: "single" | "multiple" }) {
  const items = Children.toArray(children).filter(isValidElement) as ReactElement<ItemProps>[];
  const checkable = items.some((item) => item.props.checked !== undefined);
  return <AriaMenuSection id={id} aria-label={label} className={`menu-section${layout === "list" ? " menu-section-list" : ""}`}
    selectionMode={checkable ? selectionMode : undefined}
    selectedKeys={checkable ? items.filter((item) => item.props.checked).map((item) => item.props.id) : undefined}>
    {header && <Header className={typeof header === "string" ? "menu-header ui-list-heading menu-heading" : "menu-header"}>{label && <span className="visually-hidden">{label}</span>}<div aria-hidden="true">{header}</div></Header>}
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
  trigger: ReactNode; triggerRef?: Ref<HTMLButtonElement>; triggerClassName?: string; triggerVariant?: "secondary" | "quiet"; triggerSize?: "default" | "small"; triggerTitle?: string; triggerLabel?: string;
  isOpen: boolean; onOpenChange: (open: boolean) => void;
  /** Where Tab goes: leaving the menu returns there so keys keep working. */
  tabFocusRef?: RefObject<HTMLElement | null>;
  placement?: "top end" | "bottom end";
  children: ReactNode;
};

/**
 * A menu button with a popover menu. React Aria owns roles, arrow keys,
 * typeahead, submenus, outside clicks and placement; the layer stack owns
 * Escape. A closing modal refuses focus and traps the next key, so a key or a
 * chosen item closes it at once, with no exit to wait for. Opened by a
 * pointer, nothing is highlighted until the pointer or an arrow key moves;
 * opened from the keyboard, including a shortcut that sets `isOpen`, the first
 * item is highlighted, as in native menus.
 */
export function Menu({ label, id, trigger, triggerRef, triggerClassName, triggerVariant = "secondary", triggerSize = "default", triggerTitle, triggerLabel, isOpen, onOpenChange, tabFocusRef, placement = "bottom end", children }: MenuProps) {
  const fromTrigger = useRef(false);
  const button = useRef<HTMLButtonElement | null>(null);
  // The footer's leading status row opens above its leading edge; trailing
  // footer actions keep their own placement. React Aria owns the gap and fit.
  const leadingStatus = triggerClassName?.split(/\s+/).includes("library-status-trigger");
  const renderedPlacement = useMenuPlacement(isOpen, button, leadingStatus ? "top start" : placement);
  const { list, listRef, opening } = useMenuLifecycle(isOpen);
  const wholeRows = useWholeRows(list);
  // With room for its least on neither side, it opens as a view (decision 0062).
  const fallback = useViewFallback(isOpen);
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
  useLayoutEffect(() => { if (isOpen) setSkipExit(false); }, [isOpen]);
  const leave = (to: HTMLElement | null | undefined) => {
    const previous = document.activeElement;
    flushSync(() => { setSkipExit(true); onOpenChange(false); });
    const active = document.activeElement;
    if (to?.isConnected && (active === previous || active === document.body || active === button.current || active === to)) to.focus();
  };
  const close = (event?: KeyboardEvent) => {
    // Escape leaves the innermost submenu first; React Aria owns that step.
    const inner = event?.target instanceof Element ? event.target.closest('[role="menu"]') : null;
    if (inner && inner !== list.current) return false;
    leave(event ? button.current : tabFocusRef?.current);
  };
  const frame = <div className="menu-frame" onKeyDownCapture={(event) => {
    if (isComposing(event)) { event.stopPropagation(); return; }
    // Tab leaves the entire menu, from any depth.
    if (event.key !== "Tab" || tabFocusRef === undefined) return;
    event.preventDefault();
    event.stopPropagation();
    leave(tabFocusRef.current);
  }} onKeyUpCapture={(event) => { if (isComposing(event)) event.stopPropagation(); }}>
    <MenuList key={opening} ref={listRef} id={id} className="menu scroll-area" aria-label={label} autoFocus={fromTrigger.current ? undefined : "first"}
      render={(props) => <div {...props as ComponentPropsWithRef<"div">} aria-label={label} aria-labelledby={undefined} />}>
      {children}
    </MenuList>
  </div>;
  // Outside the collection builder so submenu portals inherit the action owner.
  return <ActionFocusContext.Provider value={() => leave(tabFocusRef?.current)}><MenuTrigger isOpen={isOpen} onOpenChange={(open) => { if (!fallback.heeds(open)) return; fromTrigger.current = open; onOpenChange(open); }}>
    <Button variant={triggerVariant} size={triggerSize} aria-label={label} ref={(node) => {
      button.current = node;
      if (typeof triggerRef === "function") triggerRef(node);
      else if (triggerRef) triggerRef.current = node;
    }} className={triggerClassName}
      render={(props) => <button {...props as ComponentPropsWithRef<"button">} title={triggerTitle} aria-label={triggerLabel ?? label} />}>{trigger}</Button>
    {fallback.asView ? <MenuView label={label} onClose={close} onBack={() => leave(button.current)}>{frame}</MenuView>
      : <Floating kind="menu" open={isOpen} onClose={close} ref={wholeRows} className="menu-popover ui-overlay" anchor={button} least={menuLeast} onCramped={fallback.cramped} placement={renderedPlacement} offset={6} containerPadding={8} shouldSkipAnimation={skipExit}>
        {frame}
      </Floating>}
  </MenuTrigger></ActionFocusContext.Provider>;
}

/** A menu opened as a view of the window (decision 0062), the way the palettes
 * open: the same list and rows, as tall as the window allows, scrolling inside.
 * Esc, Back and a pick close it as they close the menu. */
function MenuView({ label, onClose, onBack, children }: { label: string; onClose: LayerClose; onBack: () => void; children: ReactNode }) {
  const layer = useLayer("palette", true, (event) => { layer.leave(); return onClose(event); });
  const titleId = useId();
  const frame = useRef<HTMLDivElement>(null);
  // The view takes the keyboard as it opens, as the menu did: the list moved
  // here from the popover, and focus left with it.
  useLayoutEffect(() => {
    if (!frame.current?.contains(document.activeElement)) frame.current?.querySelector<HTMLElement>('[role="menu"]')?.focus({ preventScroll: true });
    keepKeyboardThroughPress();
  }, []);
  return <ViewFrame title={label} titleId={titleId} frameRef={frame} role="dialog" aria-modal="true" layer={layer} back={{ label: say("palette-back"), onBack: () => { layer.leave(); onBack(); } }}>
    <div className="menu-view">{children}</div>
  </ViewFrame>;
}
