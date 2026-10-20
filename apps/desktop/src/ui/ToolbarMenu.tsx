import { useEffect, useRef, useState, type RefObject, type ComponentPropsWithRef } from "react";
import { Autocomplete, Button, Header, Input, Menu, MenuItem, MenuSection, MenuTrigger, Popover, SearchField, Text, useFilter } from "react-aria-components";
import "./ToolbarMenu.css";

export type ToolbarMenuSection = { id: string; label: string; items: { id: string; label: string; sample?: string; prose?: boolean; disabled?: boolean }[] };
type Props = {
  label: string; value: string; sections: ToolbarMenuSection[]; selectedKey: string;
  isOpen?: boolean; onOpenChange?: (open: boolean) => void;
  filterLabel: string; onAction: (key: string) => void; returnFocusRef?: RefObject<HTMLElement | null>;
};

/** A reference-independent menu. React Aria owns selection, navigation and virtual focus. */
export function ToolbarMenu({ label, value, sections, selectedKey, filterLabel, onAction, returnFocusRef, isOpen, onOpenChange }: Props) {
  const [internalOpen, setOpen] = useState(false);
  const open = isOpen ?? internalOpen;
  const composing = useRef(false);
  const focusFrame = useRef(0);
  useEffect(() => () => cancelAnimationFrame(focusFrame.current), []);
  const { contains } = useFilter({ sensitivity: "base" });
  const filtered = sections.reduce((sum, section) => sum + section.items.length, 0) > 7;
  function changeOpen(next: boolean) {
    cancelAnimationFrame(focusFrame.current);
    setOpen(next);
    onOpenChange?.(next);
    if (!next && returnFocusRef) focusFrame.current = requestAnimationFrame(() => returnFocusRef.current?.focus());
  }
  // aria-disabled keeps unavailable choices in the arrow-key order (ADR 0021).
  // Controlled selection and the action guard prevent picking them.
  const menu = <Menu className="toolbar-menu" aria-label={label} selectionMode="single" selectedKeys={[selectedKey]}
    onAction={(key) => { if (!sections.flatMap((section) => section.items).find((item) => item.id === key)?.disabled) onAction(String(key)); }}>
    {sections.map((section) => <MenuSection key={section.id} id={section.id}>
      <Header className="toolbar-menu-heading">{section.label}</Header>
      {section.items.map((item) => <MenuItem className="toolbar-menu-item" id={item.id} key={item.id} textValue={item.label} shouldCloseOnSelect={!item.disabled}
        render={(props) => <div {...props as ComponentPropsWithRef<"div">} aria-disabled={item.disabled || undefined} />}>
        {({ isSelected }) => <><span className="toolbar-menu-check" aria-hidden="true">{isSelected ? "✓" : ""}</span>
          <Text slot="label">{item.label}</Text><Text slot="description" className="toolbar-menu-sample" data-prose={item.prose || undefined}>{item.sample}</Text></>}
      </MenuItem>)}
    </MenuSection>)}
  </Menu>;
  return <span className="toolbar-menu-control" onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }}
    onKeyDownCapture={(event) => {
      if (composing.current || event.nativeEvent.isComposing || event.keyCode === 229) event.stopPropagation();
      else if (open && (event.key === "Escape" || event.key === "Tab")) { event.preventDefault(); event.stopPropagation(); changeOpen(false); }
    }}
    onKeyDown={(event) => { if (open && !event.ctrlKey && !event.metaKey) event.stopPropagation(); }}>
    <MenuTrigger isOpen={open} onOpenChange={changeOpen}>
      <Button className="toolbar-menu-button" aria-label={`${label} ${value}`}>{value}<svg aria-hidden="true" width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="m3 4.5 3 3 3-3" /></svg></Button>
      <Popover className="toolbar-menu-popover" placement="bottom end" offset={6}>
        {filtered ? <Autocomplete filter={contains}><SearchField className="toolbar-menu-filter" aria-label={filterLabel} autoFocus><Input placeholder={filterLabel} /></SearchField>{menu}</Autocomplete> : menu}
      </Popover>
    </MenuTrigger>
  </span>;
}
