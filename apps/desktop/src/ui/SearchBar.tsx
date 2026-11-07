import type { ComponentPropsWithRef, MouseEventHandler, ReactNode } from "react";
import { composing } from "../composition";
import { IconButton } from "./IconButton";
import "./SearchBar.css";

/** The picker's top band: the window's drag region, holding the mark, the search
 * field and the controls beside it. The editable field itself is not a drag region. */
export function SearchBar({ hidden, title, onMouseDown, onMouseUp, children }: {
  hidden?: boolean; title: string; children: ReactNode;
  onMouseDown?: MouseEventHandler<HTMLElement>; onMouseUp?: MouseEventHandler<HTMLElement>;
}) {
  return <header className="search-bar" hidden={hidden} data-tauri-drag-region title={title} onMouseDown={onMouseDown} onMouseUp={onMouseUp}>{children}</header>;
}

/** The large, frameless search field: a hidden label, the input, a clear button
 * while it holds text and anything that qualifies the query after it. Keys typed
 * while an input method composes stay with the field. */
export function SearchField({ id, label, clearLabel, value, onClear, trailing, ...input }: Omit<ComponentPropsWithRef<"input">, "id" | "value"> & {
  id: string; label: string; clearLabel: string; value: string; onClear: () => void; trailing?: ReactNode;
}) {
  return <>
    <label className="visually-hidden" htmlFor={id}>{label}</label>
    <div className="search-field" title=""
      onKeyDownCapture={(e) => { if (composing(e)) e.stopPropagation(); }}
      onKeyUpCapture={(e) => { if (composing(e)) e.stopPropagation(); }}>
      <div className="query-input">
        <input id={id} type="search" value={value} {...input} />
        {value !== "" && <IconButton size="large" className="search-clear" aria-label={clearLabel} onPress={onClear} />}
      </div>
      {trailing}
    </div>
  </>;
}
