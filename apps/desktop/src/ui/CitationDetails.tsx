import { Input, Label, Radio, RadioGroup, Text, TextField } from "react-aria-components";
import { useId, type RefObject } from "react";
import type { ReferenceForm } from "../bindings";
import { citationDetailsText as text } from "../messages";
import "./CitationDetails.css";

/** The same per-reference editor wherever a citation is being prepared. */
export function CitationDetails({ label, page, pageRef, form, autoFocus = false, onPageChange, onFormChange }: {
  label: string; page: string; form: ReferenceForm;
  pageRef?: RefObject<HTMLInputElement | null>;
  autoFocus?: boolean;
  onPageChange: (value: string) => void;
  onFormChange: (value: ReferenceForm) => void;
}) {
  const unavailable = useId();
  return <section className="citation-details" aria-label={label}>
    <TextField className="citation-page" value={page} onChange={onPageChange}>
      <Label>{text.page}</Label>
      <Input ref={pageRef} autoFocus={autoFocus} placeholder={text.pageExample} />
      <Text slot="description">{text.pageHint}</Text>
    </TextField>
    <RadioGroup className="citation-forms" aria-label={text.form} orientation="vertical" value={form} onChange={(value) => {
      if (value === "normal" || value === "narrative") onFormChange(value);
    }}>
      {text.forms.map((option) => <Radio key={option.value} className="citation-form" value={option.value} aria-label={option.label}
        isDisabled={option.value === "year-only"} aria-describedby={option.value === "year-only" ? unavailable : undefined}>
        <span className="citation-form-check" aria-hidden="true">{form === option.value ? "✓" : ""}</span><span>{option.label}</span>
      </Radio>)}
    </RadioGroup>
    <p id={unavailable} className="citation-form-reason">Year only isn’t available yet.</p>
  </section>;
}
