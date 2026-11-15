import type { ComponentPropsWithRef, ReactNode } from "react";
import { composing } from "../composition";
import { Band } from "./Band";
import "./SearchBar.css";

/** The picker's top band (ui/Band): the window's drag region, holding the mark in
 * the icon column, the search field on the label column and the controls beside
 * it. The mark and the band's empty room move the window, and double-clicking
 * them returns it to its place; the editable field itself never moves it. */
export function SearchBar({ hidden, title, mark, onResetPosition, children }: {
  hidden?: boolean; title: string; mark: ReactNode; children: ReactNode;
  onResetPosition?: () => void;
}) {
  return <Band as="header" className="search-bar" icon={mark} hidden={hidden} drag onDragReset={onResetPosition} title={title}>{children}</Band>;
}

/** The large, frameless search field: a hidden label, the input and anything
 * that qualifies the query after it. Esc clears. Keys typed
 * while an input method composes stay with the field. */
export function SearchField({ id, label, value, trailing, ...input }: Omit<ComponentPropsWithRef<"input">, "id" | "value"> & {
  id: string; label: string; value: string; trailing?: ReactNode;
}) {
  return <>
    <label className="visually-hidden" htmlFor={id}>{label}</label>
    <div className="search-field" title=""
      onKeyDownCapture={(e) => { if (composing(e)) e.stopPropagation(); }}
      onKeyUpCapture={(e) => { if (composing(e)) e.stopPropagation(); }}>
      <div className="query-input">
        <input id={id} type="search" value={value} {...input} />
      </div>
      {trailing}
    </div>
  </>;
}
