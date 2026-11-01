import "./SettingsGroup.css";
import { useRef } from "react";
import { Icon } from "../icons";
import { pickerText } from "../messages";
import { IconButton } from "./IconButton";

/** The same labelled search field in the sidebar and shortcut catalogue. */
export function SettingsSearch({ label, value, onChange, className = "" }: {
  label: string; value: string; onChange: (value: string) => void; className?: string;
}) {
  const input = useRef<HTMLInputElement>(null);
  return <div className={`ui-settings-search ${className}`}>
    <Icon name="find" className="ui-settings-search-glyph" />
    <input ref={input} type="search" aria-label={label} placeholder={label} value={value} onChange={(event) => onChange(event.target.value)} />
    {value && <IconButton className="ui-settings-search-clear" aria-label={pickerText.clearSearch}
      onPress={() => { onChange(""); input.current?.focus(); }} />}
  </div>;
}
