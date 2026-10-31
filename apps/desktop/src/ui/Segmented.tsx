import { Radio, RadioGroup } from "react-aria-components";
import "./Segmented.css";

/** A few exclusive choices side by side, such as row density. Arrow keys move
 * the choice, as in a native segmented control. */
export function Segmented<T extends string>({ label, value, options, onChange }: {
  label: string; value: T;
  options: readonly { value: T; label: string; isDisabled?: boolean }[];
  onChange: (value: T) => void;
}) {
  return (
    <RadioGroup className="ui-segmented" aria-label={label} orientation="horizontal" value={value} onChange={(next) => onChange(next as T)}>
      {options.map((option) => <Radio key={option.value} value={option.value} isDisabled={option.isDisabled} className="ui-segment">{option.label}</Radio>)}
    </RadioGroup>
  );
}
