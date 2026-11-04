import { Input, Label, TextField as AriaTextField } from "react-aria-components";
import "./TextField.css";

/** A labelled single-line field; forms own validation and saving. */
export function TextField({ label, value, onChange, isDisabled = false, autoFocus = false, maxLength }: {
  label: string; value: string; onChange: (value: string) => void;
  isDisabled?: boolean; autoFocus?: boolean; maxLength?: number;
}) {
  return <AriaTextField className="ui-text-field" value={value} onChange={onChange} isDisabled={isDisabled}>
    <Label>{label}</Label>
    <Input autoFocus={autoFocus} maxLength={maxLength} />
  </AriaTextField>;
}
