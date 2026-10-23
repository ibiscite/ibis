import { useCallback, useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type RefObject, type ComponentPropsWithRef } from "react";
import { composing as isComposing } from "../composition";
import { flushSync } from "react-dom";
import { Autocomplete, Header, Input, Menu, MenuItem, MenuSection, MenuTrigger, Popover, SearchField, Text, useFilter } from "react-aria-components";
import { Button } from "./Button";
import { MenuCheck, useMenuBoundary, useMenuPlacement } from "./Menu";
import { currentPlatform } from "../platform";
import { useMenuLifecycle } from "./useMenuLifecycle";
import "./ToolbarMenu.css";

export type ToolbarMenuSection = { id: string; label: string; items: { id: string; label: string; sample?: string; prose?: boolean; disabled?: boolean }[] };
export type ToolbarMenuCommand = { run: () => void; isCurrent: () => boolean };
type Props = {
  label: string; value: string; sections: ToolbarMenuSection[]; selectedKey: string;
  isOpen?: boolean; onOpenChange?: (open: boolean) => void;
  filterLabel: string; onAction: (key: string) => void; returnFocusRef?: RefObject<HTMLElement | null>;
  /** Close and return focus before running an application command. */
  onKeyCommand?: (event: KeyboardEvent) => ToolbarMenuCommand | undefined;
};

/** A reference-independent menu. React Aria owns selection, navigation and virtual focus. */
export function ToolbarMenu({ label, value, sections, selectedKey, filterLabel, onAction, returnFocusRef, isOpen, onOpenChange, onKeyCommand }: Props) {
  const [internalOpen, setOpen] = useState(false);
  const [skipExit, setSkipExit] = useState(false);
  const open = isOpen ?? internalOpen;
  const boundary = useMenuBoundary(open, returnFocusRef);
  const { listRef, opening, retryFocus, cancelFocusRetry } = useMenuLifecycle(open);
  const composing = useRef(false);
  const button = useRef<HTMLButtonElement>(null);
  const renderedPlacement = useMenuPlacement(open, button, "bottom end");
  const popover = useRef<HTMLElement | null>(null);
  const generation = useRef(0);
  const pending = useRef<{ target: HTMLElement; closing: HTMLElement | null; token: number; command?: ToolbarMenuCommand } | null>(null);
  const cancel = useCallback(() => {
    generation.current += 1;
    pending.current = null;
    cancelFocusRetry();
  }, [cancelFocusRetry]);
  const restoreFocus = useCallback(() => {
    const request = pending.current;
    if (!request || popover.current || request.closing?.isConnected) return;
    pending.current = null;
    const current = () => request.token === generation.current && request.command?.isCurrent() !== false;
    const active = document.activeElement;
    if (!current() || !request.target.isConnected || (active !== document.body && active !== button.current && active !== request.target && !request.closing?.contains(active))) return;
    request.target.focus();
    if (current() && document.activeElement === request.target) request.command?.run();
  }, []);
  const popoverRef = useCallback((node: HTMLElement | null) => {
    popover.current = node;
    if (!node && pending.current) retryFocus(restoreFocus);
  }, [restoreFocus, retryFocus]);
  useLayoutEffect(() => { if (open) { setSkipExit(false); cancel(); } }, [open, cancel]);
  useEffect(() => {
    const hidden = () => { if (document.hidden) cancel(); };
    document.addEventListener("keydown", cancel, true);
    document.addEventListener("pointerdown", cancel, true);
    document.addEventListener("visibilitychange", hidden);
    window.addEventListener("blur", cancel);
    return () => {
      cancel();
      document.removeEventListener("keydown", cancel, true);
      document.removeEventListener("pointerdown", cancel, true);
      document.removeEventListener("visibilitychange", hidden);
      window.removeEventListener("blur", cancel);
    };
  }, [cancel]);
  const { contains } = useFilter({ sensitivity: "base" });
  const filtered = sections.reduce((sum, section) => sum + section.items.length, 0) > 7;
  function changeOpen(next: boolean, command?: ToolbarMenuCommand, immediate = false) {
    cancel();
    if (!next && immediate) {
      // Selection hands typing back now. An exiting modal traps the next key
      // before the deferred focus return can reach Search.
      const previous = document.activeElement;
      flushSync(() => { setSkipExit(true); setOpen(false); onOpenChange?.(false); });
      const target = returnFocusRef?.current;
      const active = document.activeElement;
      if (target?.isConnected && (active === previous || active === document.body || active === button.current || active === target)) target.focus();
      return;
    }
    if (!next && returnFocusRef?.current) pending.current = {
      target: returnFocusRef.current, closing: popover.current, token: generation.current, command,
    };
    setOpen(next);
    onOpenChange?.(next);
  }
  // aria-disabled keeps unavailable choices in the arrow-key order (ADR 0021).
  // Controlled selection and the action guard prevent picking them.
  const menu = <Menu key={opening} ref={listRef} className="toolbar-menu" aria-label={label} selectionMode="single" selectedKeys={[selectedKey]}
    onAction={(key) => {
      if (sections.flatMap((section) => section.items).find((item) => item.id === key)?.disabled) return;
      changeOpen(false, undefined, true);
      onAction(String(key));
    }}>
    {sections.map((section) => <MenuSection key={section.id} id={section.id} className="menu-section">
      <Header className="toolbar-menu-heading">{section.label}</Header>
      {section.items.map((item) => <MenuItem className="menu-item" id={item.id} key={item.id} textValue={item.label} shouldCloseOnSelect={false}
        render={(props) => <div {...props as ComponentPropsWithRef<"div">} aria-disabled={item.disabled || undefined} />}>
        {({ isSelected }) => <><MenuCheck checked={isSelected} />
          <Text slot="label" className="menu-label">{item.label}</Text><Text slot="description" className="toolbar-menu-sample" data-prose={item.prose || undefined}>{item.sample}</Text></>}
      </MenuItem>)}
    </MenuSection>)}
  </Menu>;
  return <span className="toolbar-menu-control" onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }}
    onKeyDownCapture={(event) => {
      if (isComposing(event, composing.current)) event.stopPropagation();
      else if (open || popover.current) {
        const command = onKeyCommand?.(event);
        if (command || event.key === "Escape" || event.key === "Tab") { event.preventDefault(); event.stopPropagation(); changeOpen(false, command); }
      }
    }}
    onKeyDown={(event) => { if ((open || popover.current) && !event.ctrlKey && !event.metaKey) event.stopPropagation(); }}>
    <MenuTrigger isOpen={open} onOpenChange={changeOpen}>
      <Button ref={button} className="toolbar-menu-button" aria-label={`${label} ${value}`}>{value}<PopupIndicator /></Button>
      <Popover ref={popoverRef} className="menu-popover toolbar-menu-popover" placement={renderedPlacement} offset={4} containerPadding={8} shouldSkipAnimation={skipExit} {...boundary}>
        {filtered ? <Autocomplete key={opening} filter={contains}><SearchField className="toolbar-menu-filter" aria-label={filterLabel} autoFocus><Input placeholder={filterLabel} /></SearchField>{menu}</Autocomplete> : menu}
      </Popover>
    </MenuTrigger>
  </span>;
}

/** Bundled masters for each system's popup indicator; opening never turns it. */
function PopupIndicator() {
  const platform = currentPlatform();
  const [viewBox, path] = platform === "macos" ? ["0 0 8 12", "m1 4 3-3 3 3m-6 4 3 3 3-3"]
    : platform === "windows" ? ["0 0 12 12", "m3 4.5 3 3 3-3"] : ["0 0 16 16", "m4 6 4 4 4-4"];
  return <svg className={`popup-indicator popup-indicator-${platform}`} aria-hidden="true" focusable="false"
    viewBox={viewBox} fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round"><path d={path} /></svg>;
}
