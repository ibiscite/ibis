import "./SettingsSearch.css";
import { useRef, type KeyboardEventHandler, type RefObject } from "react";
import { composing } from "../composition";
import { Icon } from "../icons";

/** The same labelled search field in the sidebar and shortcut catalogue. */
export function SettingsSearch({ label, value, onChange, onSubmit, inputRef, onKeyDown, className = "" }: {
  label: string; value: string; onChange: (value: string) => void; onSubmit?: () => void; className?: string;
  inputRef?: RefObject<HTMLInputElement | null>; onKeyDown?: KeyboardEventHandler<HTMLInputElement>;
}) {
  const localInput = useRef<HTMLInputElement>(null);
  const input = inputRef ?? localInput;
  return <div className={`ui-settings-search ${className}`}>
    <Icon name="find" className="ui-settings-search-glyph" />
    <input ref={input} type="search" aria-label={label} placeholder={label} value={value} onChange={(event) => onChange(event.target.value)}
      onKeyDown={(event) => {
        if (composing(event.nativeEvent)) { event.stopPropagation(); return; }
        onKeyDown?.(event);
        if (event.defaultPrevented) return;
        if (event.key === "Escape" && (value || event.repeat)) {
          event.preventDefault(); event.stopPropagation();
          if (!event.repeat) onChange("");
        } else if (event.key === "Enter") onSubmit?.();
      }} />
  </div>;
}
