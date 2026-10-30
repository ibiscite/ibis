import type { ReactNode } from "react";
import { Radio, RadioGroup } from "react-aria-components";
import "./PictureChoice.css";

/** Shared picture radios: tinted selection and its ring persist; keyboard focus draws farther inward. */
export function PictureChoice<T extends string>({ label, description, value, options, onChange, dots = false, isDisabled = false }: {
  label: string; description?: string; value: T; dots?: boolean; isDisabled?: boolean;
  options: readonly { value: T; label: string; picture?: ReactNode; isDisabled?: boolean }[];
  onChange: (value: T) => void;
}) {
  return <RadioGroup className="ui-picture-choices" data-dots={dots || undefined} aria-label={label} aria-describedby={description}
    orientation="horizontal" value={value} isDisabled={isDisabled} onChange={(next) => onChange(next as T)}>
    {options.map((option) => <Radio className="ui-picture-choice" key={option.value} value={option.value} isDisabled={option.isDisabled} aria-label={option.label}>
      {option.picture}
      <span className={dots ? "ui-visually-hidden" : undefined}>{option.label}</span>
    </Radio>)}
  </RadioGroup>;
}
