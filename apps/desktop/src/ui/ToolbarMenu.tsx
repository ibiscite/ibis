import { useId, useLayoutEffect, useRef, useState, type ReactNode, type RefObject, type ComponentPropsWithRef } from "react";
import { composing as isComposing } from "../composition";
import { flushSync } from "react-dom";
import { Autocomplete, Header, Input, Menu, MenuItem, MenuSection, MenuTrigger, Popover, SearchField, Text, useFilter } from "react-aria-components";
import { PopupButton } from "./PopupButton";
import { MenuCheck, useMenuBoundary, useMenuPlacement, MenuRow, useWholeRows } from "./Menu";
import { useLayer } from "./layers";
import { useMenuLifecycle } from "./useMenuLifecycle";
import "./ToolbarMenu.css";

/** A `note` says something quietly: on an item, why it is dimmed, under its label;
 * on a section, what holds for all of it. */
export type ToolbarMenuSection = { id: string; label?: string; note?: string; items: { id: string; label: string; sample?: string; prose?: boolean; note?: string; disabled?: boolean; command?: boolean }[] };
/** A pick, a dismissal by Esc, Tab or a click, or a command clearing the way. */
export type MenuCloseReason = "action" | "dismiss" | "command";
type Props = {
  buttonRef?: RefObject<HTMLButtonElement | null>; buttonShortcut?: string; disabled?: boolean; label: string; value: string; sections: ToolbarMenuSection[]; selectedKey: string;
  /** One plain line above the choices, read with the menu. */
  note?: string;
  /** A pop-up that draws more than its value, such as the shelf's count. */
  button?: { className: string; label: string; title?: string; content: ReactNode };
  placement?: "top end" | "bottom end";
  isOpen?: boolean; onOpenChange?: (open: boolean, reason?: MenuCloseReason) => void;
  filterLabel?: string; onAction: (key: string) => void; returnFocusRef?: RefObject<HTMLElement | null>;
};

/** A reference-independent menu. React Aria owns selection, navigation and virtual focus. */
export function ToolbarMenu({ buttonRef, buttonShortcut, disabled, label, value, note, sections, selectedKey, button: drawn, placement = "bottom end", filterLabel = label, onAction, returnFocusRef, isOpen, onOpenChange }: Props) {
  const [internalOpen, setOpen] = useState(false);
  const [skipExit, setSkipExit] = useState(false);
  const open = isOpen ?? internalOpen;
  const internalButton = useRef<HTMLButtonElement>(null);
  const button = buttonRef ?? internalButton;
  const boundary = useMenuBoundary(open, returnFocusRef, button);
  const { list, listRef, opening } = useMenuLifecycle(open);
  const wholeRows = useWholeRows(list);
  const noteId = useId();
  const composing = useRef(false);
  const renderedPlacement = useMenuPlacement(open, button, placement);
  useLayoutEffect(() => { if (open) setSkipExit(false); }, [open]);
  const { contains } = useFilter({ sensitivity: "base" });
  const filtered = sections.reduce((sum, section) => sum + section.items.length, 0) > 7;
  function changeOpen(next: boolean, reason: MenuCloseReason = "dismiss") {
    if (next) {
      setOpen(true);
      onOpenChange?.(true);
      return;
    }
    // Closing hands typing back now. An exiting modal refuses focus and traps
    // the next key, so nothing waits for its exit.
    const previous = document.activeElement;
    flushSync(() => { setSkipExit(true); setOpen(false); onOpenChange?.(false, reason); });
    const target = returnFocusRef?.current ?? button.current;
    const active = document.activeElement;
    if (target?.isConnected && (active === previous || active === document.body || active === button.current || active === target)) target.focus();
  }
  useLayer("menu", open, (event) => changeOpen(false, event ? "dismiss" : "command"));
  // aria-disabled keeps unavailable choices in the arrow-key order (ADR 0021).
  // Controlled selection and the action guard prevent picking them.
  const menu = <Menu key={opening} ref={listRef} className="toolbar-menu scroll-area" aria-label={label} aria-describedby={note ? noteId : undefined} selectionMode="single" selectedKeys={[selectedKey]}
    onAction={(key) => {
      if (disabled) return;
      if (sections.flatMap((section) => section.items).find((item) => item.id === key)?.disabled) return;
      changeOpen(false, "action");
      onAction(String(key));
    }}>
    {sections.map((section) => <MenuSection key={section.id} id={section.id} className="menu-section">
      {section.label && <Header className="menu-heading">{section.label}</Header>}
      {section.note && <Header className="toolbar-menu-note">{section.note}</Header>}
      {section.items.map((item) => <MenuItem className="menu-item" id={item.id} key={item.id} textValue={item.label} shouldCloseOnSelect={false}
        render={(props) => <MenuRow {...props as ComponentPropsWithRef<"div">} itemId={item.id} aria-disabled={disabled || item.disabled || undefined}
          {...item.command ? { role: "menuitem", "aria-checked": undefined } : {}} />}>
        {({ isSelected }) => <><MenuCheck checked={!item.command && isSelected} />
          <Text slot="label" className="menu-label">{item.label}</Text><Text slot="description" className="toolbar-menu-sample" data-prose={item.prose || undefined} data-note={item.note ? "" : undefined}>{item.note ?? item.sample}</Text></>}
      </MenuItem>)}
    </MenuSection>)}
  </Menu>;
  return <span className="toolbar-menu-control" onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }}
    onKeyDownCapture={(event) => {
      if (isComposing(event, composing.current)) event.stopPropagation();
      else if (open && event.key === "Tab") { event.preventDefault(); event.stopPropagation(); changeOpen(false); }
    }}>
    <MenuTrigger isOpen={open} onOpenChange={(next) => changeOpen(next)}>
      <PopupButton ref={button} aria-keyshortcuts={buttonShortcut} isDisabled={disabled} className={`toolbar-menu-button ${drawn?.className ?? ""}`} size={drawn ? "small" : undefined} title={drawn?.title} aria-label={drawn?.label ?? `${label} ${value}`}>{drawn?.content ?? value}</PopupButton>
      <Popover ref={wholeRows} className="menu-popover toolbar-menu-popover ui-overlay" placement={renderedPlacement} offset={4} containerPadding={8} shouldSkipAnimation={skipExit} {...boundary}>
        {note && <p id={noteId} className="toolbar-menu-note">{note}</p>}
        {filtered ? <Autocomplete key={opening} filter={contains}><SearchField className="toolbar-menu-filter" aria-label={filterLabel} autoFocus><Input placeholder={filterLabel} aria-describedby={note ? noteId : undefined} /></SearchField>{menu}</Autocomplete> : menu}
      </Popover>
    </MenuTrigger>
  </span>;
}

