import { Input, Label, Radio, RadioGroup, Text, TextField } from "react-aria-components";
import { useId, type RefObject } from "react";
import type { ReferenceDetails, ReferenceForm } from "../bindings";
import { citationDetailsText as text } from "../messages";
import "./CitationDetails.css";

/** The same per-reference editor wherever a citation is being prepared. */
export function CitationDetails({ label, page, pageRef, form, choices, autoFocus = false, onPageChange, onFormChange }: {
  label: string; page: string; form: ReferenceForm;
  pageRef?: RefObject<HTMLInputElement | null>;
  autoFocus?: boolean;
  choices?: ReferenceDetails | "failed";
  onPageChange: (value: string) => void;
  onFormChange: (value: ReferenceForm) => void;
}) {
  const descriptions = useId();
  return <section className="citation-details" aria-label={label}>
    <TextField className="citation-page" value={page} onChange={onPageChange}>
      <Label>{text.page}</Label>
      <Input ref={pageRef} autoFocus={autoFocus} placeholder={text.pageExample} />
      <Text slot="description">{text.pageHint}</Text>
    </TextField>
    <RadioGroup className="citation-forms" aria-label={text.form} orientation="vertical" value={form} onChange={(value) => {
      if (value === "normal" || value === "narrative" || value === "year-only") onFormChange(value);
    }}>
      {text.forms.map((option) => {
        const choice = typeof choices === "object" ? choices.forms.find((choice) => choice.form === option.value) : undefined;
        const reason = choice?.reason ?? (!choice ? choices === "failed" ? "Examples are unavailable. Close and reopen the menu to try again." : "Preparing examples…" : null);
        const description = `${descriptions}-${option.value}`;
        return <Radio key={option.value} className="citation-form" value={option.value} aria-label={option.label}
          isDisabled={Boolean(reason)} aria-describedby={description}>
          <span className="citation-form-check" aria-hidden="true">{form === option.value ? "✓" : ""}</span>
          <span className="citation-form-body"><span>{option.label}</span>
            <span id={description} className="citation-form-example">{reason ?? choice?.example}</span></span>
        </Radio>;
      })}
    </RadioGroup>
  </section>;
}
