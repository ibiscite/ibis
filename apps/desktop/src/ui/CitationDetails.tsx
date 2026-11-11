import { Input, Label, Text, TextField } from "react-aria-components";
import { useRef, type KeyboardEvent, type RefObject } from "react";
import type { ReferenceDetails, ReferenceForm } from "../bindings";
import { composing } from "../composition";
import { say } from "../i18n";
import { ToolbarMenu } from "./ToolbarMenu";
import "./CitationDetails.css";

/** Each form by its one name, from the catalogue. */
const FORMS = [
  { value: "normal", label: say("form-normal") },
  { value: "narrative", label: say("form-narrative") },
  { value: "author-only", label: say("form-author-only") },
  { value: "year-only", label: say("form-year-only") },
  { value: "full", label: say("form-full") },
] as const;

/** The same per-reference fields wherever a citation is being prepared. Only the
 * engine says a form cannot be used: until it answers, every form can be tried,
 * so a refused citation can always be corrected from here. */
export function CitationDetails({ page, form, prefix, suffix, pageRef, choices, autoFocus = false, onPageChange, onFormChange, onPrefixChange, onSuffixChange, onSubmit }: {
  page: string; form: ReferenceForm; prefix: string; suffix: string;
  pageRef?: RefObject<HTMLInputElement | null>;
  autoFocus?: boolean;
  choices?: ReferenceDetails | "failed";
  onPageChange: (value: string) => void;
  onFormChange: (value: ReferenceForm) => void;
  onPrefixChange: (value: string) => void;
  onSuffixChange: (value: string) => void;
  /** Enter in a text field does what it does in Search. */
  onSubmit?: () => void;
}) {
  const formButton = useRef<HTMLButtonElement>(null);
  const submit = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter" && !event.repeat && !composing(event)) { event.preventDefault(); onSubmit?.(); }
  };
  const answer = typeof choices === "object" ? choices : undefined;
  // The engine answers null where no name form applies, as for Keys only.
  const current = answer ? answer.current_form : form;
  // No form to choose: one quiet line says why, in place of the pop-up.
  const reasons = new Set(FORMS.map((option) => answer?.forms.find((choice) => choice.form === option.value)?.reason));
  const shared = reasons.size === 1 ? [...reasons][0] : null;
  return <div className="citation-details">
    {shared ? <p className="citation-note">{shared}</p> : <div className="citation-field">
      <span className="citation-label" aria-hidden="true">{say("citation-details-form")}</span>
      <ToolbarMenu buttonRef={formButton} returnFocusRef={formButton} placement="bottom start" label={say("citation-details-form")} value={FORMS.find((option) => option.value === current)?.label ?? say("citation-details-none")} selectedKey={current ?? ""}
        sections={[{ id: "forms", note: choices === "failed" ? say("citation-details-examples-unavailable") : undefined, items: FORMS.map((option) => {
          const choice = answer?.forms.find((choice) => choice.form === option.value);
          return { id: option.value, label: option.label, sample: choice?.example ?? undefined, note: choice?.reason ?? undefined, disabled: Boolean(choice?.reason) };
        }) }]} onAction={(key) => {
          const chosen = FORMS.find((option) => option.value === key);
          if (chosen) onFormChange(chosen.value);
        }} />
    </div>}
    <TextField className="citation-field" value={page} onChange={onPageChange}>
      <Label className="citation-label">{say("citation-details-page")}</Label>
      <Input ref={pageRef} autoFocus={autoFocus} placeholder={say("citation-details-page-placeholder")} onKeyDown={submit} />
      <Text slot="description" className="visually-hidden">{say("citation-details-page-hint")}</Text>
    </TextField>
    <TextField className="citation-field" value={prefix} onChange={onPrefixChange}>
      <Label className="citation-label">{say("citation-details-prefix")}</Label>
      <Input onKeyDown={submit} />
    </TextField>
    <TextField className="citation-field" value={suffix} onChange={onSuffixChange}>
      <Label className="citation-label">{say("citation-details-suffix")}</Label>
      <Input onKeyDown={submit} />
    </TextField>
  </div>;
}
