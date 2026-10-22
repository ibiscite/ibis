import { Children, createContext, isValidElement, useCallback, useContext, useEffect, useLayoutEffect, useRef, useState, type ComponentPropsWithRef, type ReactElement, type ReactNode, type Ref, type RefObject } from "react";
import { Button, Header, Keyboard, Menu as AriaMenu, MenuItem as AriaMenuItem, MenuSection as AriaMenuSection, MenuTrigger, Popover, Separator } from "react-aria-components";
import "./Menu.css";

const ActionFocusContext = createContext<(() => void) | null>(null);

type ItemProps = {
  id: string; label: string; title?: string; keys?: string;
  /** A toggle: shows a check in the leading column and is a menuitemcheckbox. */
  checked?: boolean;
  /** Stays focusable and is announced as unavailable, but does nothing (ADR 0021). */
  disabled?: boolean;
  onAction: () => void;
};

/** The leading check column; a drawn check, so it adds no text to the row. */
export function MenuCheck({ checked }: { checked: boolean }) {
  return <span className="menu-check" aria-hidden="true">
    {checked && <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><path d="m2.5 6.5 2.5 2.5 4.5-6" /></svg>}
  </span>;
}

/** One row: a leading check column, the label, and an optional shortcut. */
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
    render={(props) => <div {...props as ComponentPropsWithRef<"div">} title={title} aria-disabled={disabled || undefined} />}>
    {({ isSelected }) => <>
      <MenuCheck checked={isSelected} />
      <span className="menu-label">{label}</span>
      {keys && <Keyboard className="menu-keys" aria-hidden="true">{keys}</Keyboard>}
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
  const list = useRef<HTMLDivElement>(null);
  const popover = useRef<HTMLElement | null>(null);
  const pendingFocus = useRef<HTMLElement | null>(null);
  const focusFrame = useRef(0);
  const closingList = useRef<HTMLDivElement | null>(null);
  const [opening, setOpening] = useState(0);
  const listRef = useCallback((node: HTMLDivElement | null) => {
    list.current = node;
    if (!node) closingList.current = null;
  }, []);
  useEffect(() => { if (!isOpen) fromTrigger.current = false; }, [isOpen]);
  useLayoutEffect(() => {
    if (!isOpen) closingList.current = list.current;
    else {
      // An overlapping reopen retains the popover, but its collection has
      // already autofocused. Restart that collection so React Aria applies
      // the new pointer/keyboard focus strategy, including clearing old rows.
      if (closingList.current && closingList.current === list.current) setOpening((value) => value + 1);
      closingList.current = null;
    }
  }, [isOpen]);
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
  }, [isOpen]);
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
    if (!node && pendingFocus.current) focusFrame.current = requestAnimationFrame(restoreFocus);
  }, [restoreFocus]);
  useLayoutEffect(() => {
    if (isOpen) {
      pendingFocus.current = null;
      cancelAnimationFrame(focusFrame.current);
    } else restoreFocus();
  }, [isOpen, restoreFocus]);
  useEffect(() => {
    // Any later user input supersedes the closing action's destination.
    const cancel = () => { pendingFocus.current = null; };
    document.addEventListener("pointerdown", cancel, true);
    document.addEventListener("keydown", cancel, true);
    return () => {
      cancel();
      cancelAnimationFrame(focusFrame.current);
      document.removeEventListener("pointerdown", cancel, true);
      document.removeEventListener("keydown", cancel, true);
    };
  }, []);
  const leave = (to: HTMLElement | null | undefined) => {
    pendingFocus.current = to ?? null;
    onOpenChange(false);
  };
  return <MenuTrigger isOpen={isOpen} onOpenChange={(open) => { fromTrigger.current = open; onOpenChange(open); }}>
    <Button ref={(node) => {
      button.current = node;
      if (typeof triggerRef === "function") triggerRef(node);
      else if (triggerRef) triggerRef.current = node;
    }} className={triggerClassName}
      render={(props) => <button {...props as ComponentPropsWithRef<"button">} title={triggerTitle} aria-keyshortcuts={triggerKeys} />}>{trigger}</Button>
    <Popover ref={popoverRef} className="menu-popover" placement={placement} offset={4} containerPadding={8}>
      <div className="menu-frame" onKeyDownCapture={(event) => {
        if (event.nativeEvent.isComposing) return;
        const to = event.key === "Escape" ? button.current : event.key === "Tab" ? tabFocusRef?.current : undefined;
        if (to === undefined) return;
        event.preventDefault();
        event.stopPropagation();
        leave(to);
      }}>
        <AriaMenu key={opening} ref={listRef} id={id} className="menu" aria-label={label} autoFocus={fromTrigger.current ? undefined : "first"}>
          <ActionFocusContext.Provider value={() => { pendingFocus.current = tabFocusRef?.current ?? null; }}>
            {children}
          </ActionFocusContext.Provider>
        </AriaMenu>
      </div>
    </Popover>
  </MenuTrigger>;
}
