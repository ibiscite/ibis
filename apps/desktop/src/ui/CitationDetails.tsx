import { Input, Label, Text, TextField } from "react-aria-components";
import { useRef, type RefObject } from "react";
import type { ReferenceDetails, ReferenceForm } from "../bindings";
import { composing } from "../composition";
import { citationDetailsText as text } from "../messages";
import { ToolbarMenu } from "./ToolbarMenu";
import "./CitationDetails.css";

/** The same per-reference fields wherever a citation is being prepared. Only the
 * engine says a form cannot be used: until it answers, every form can be tried,
 * so a refused citation can always be corrected from here. */
export function CitationDetails({ page, form, pageRef, choices, autoFocus = false, onPageChange, onFormChange, onSubmit }: {
  page: string; form: ReferenceForm;
  pageRef?: RefObject<HTMLInputElement | null>;
  autoFocus?: boolean;
  choices?: ReferenceDetails | "failed";
  onPageChange: (value: string) => void;
  onFormChange: (value: ReferenceForm) => void;
  /** Enter in Page does what it does in Search. */
  onSubmit?: () => void;
}) {
  const formButton = useRef<HTMLButtonElement>(null);
  const answer = typeof choices === "object" ? choices : undefined;
  // The engine answers null where no name form applies, as for Keys only.
  const current = answer ? answer.current_form : form;
  // No form to choose: one quiet line says why, in place of the pop-up.
  const reasons = new Set(text.forms.map((option) => answer?.forms.find((choice) => choice.form === option.value)?.reason));
  const shared = reasons.size === 1 ? [...reasons][0] : null;
  return <div className="citation-details">
    {shared ? <p className="citation-note">{shared}</p> : <div className="citation-field">
      <span className="citation-label" aria-hidden="true">{text.form}</span>
      <ToolbarMenu buttonRef={formButton} returnFocusRef={formButton} placement="bottom start" label={text.form} value={text.forms.find((option) => option.value === current)?.label ?? text.none} selectedKey={current ?? ""}
        sections={[{ id: "forms", note: choices === "failed" ? text.examplesUnavailable : undefined, items: text.forms.map((option) => {
          const choice = answer?.forms.find((choice) => choice.form === option.value);
          return { id: option.value, label: option.label, sample: choice?.example ?? undefined, note: choice?.reason ?? undefined, disabled: Boolean(choice?.reason) };
        }) }]} onAction={(key) => {
          const chosen = text.forms.find((option) => option.value === key);
          if (chosen) onFormChange(chosen.value);
        }} />
    </div>}
    <TextField className="citation-field" value={page} onChange={onPageChange}>
      <Label className="citation-label">{text.page}</Label>
      <Input ref={pageRef} autoFocus={autoFocus} placeholder={text.pagePlaceholder} onKeyDown={(event) => {
        if (event.key === "Enter" && !event.repeat && !composing(event)) { event.preventDefault(); onSubmit?.(); }
      }} />
      <Text slot="description" className="visually-hidden">{text.pageHint}</Text>
    </TextField>
  </div>;
}
