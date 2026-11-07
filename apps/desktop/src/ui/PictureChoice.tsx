import type { CSSProperties, ReactNode } from "react";
import { Radio, RadioGroup } from "react-aria-components";
import "./PictureChoice.css";

/** Shared picture radios: tinted selection and its ring persist; keyboard focus draws farther inward. */
export function PictureChoice<T extends string>({ label, description, value, options, onChange, isDisabled = false }: {
  label: string; description?: string; value: T; isDisabled?: boolean;
  options: readonly { value: T; label: string; picture?: ReactNode; isDisabled?: boolean }[];
  onChange: (value: T) => void;
}) {
  return <RadioGroup className="ui-picture-choices" aria-label={label} aria-describedby={description}
    style={{ "--choice-columns": options.length } as CSSProperties}
    orientation="horizontal" value={value} isDisabled={isDisabled} onChange={(next) => onChange(next as T)}>
    {options.map((option) => <Radio className="ui-picture-choice" key={option.value} value={option.value} isDisabled={option.isDisabled} aria-label={option.label}>
      {option.picture && <span className="ui-picture-choice-art" title={option.label}>{option.picture}</span>}
      <span className="ui-picture-choice-label">{option.label}</span>
    </Radio>)}
  </RadioGroup>;
}
