import { Input, Label, Text, TextField } from "react-aria-components";
import type { RefObject } from "react";
import type { ReferenceForm } from "../bindings";
import { citationDetailsText as text } from "../messages";
import { Segmented } from "./Segmented";
import "./CitationDetails.css";

/** The same per-reference editor wherever a citation is being prepared. */
export function CitationDetails({ label, page, pageRef, form, autoFocus = false, onPageChange, onFormChange }: {
  label: string; page: string; form: ReferenceForm;
  pageRef?: RefObject<HTMLInputElement | null>;
  autoFocus?: boolean;
  onPageChange: (value: string) => void;
  onFormChange: (value: ReferenceForm) => void;
}) {
  return <section className="citation-details" aria-label={label}>
    <TextField className="citation-page" value={page} onChange={onPageChange}>
      <Label>{text.page}</Label>
      <Input ref={pageRef} autoFocus={autoFocus} placeholder={text.pageExample} />
      <Text slot="description">{text.pageHint}</Text>
    </TextField>
    <Segmented label={text.form} value={form} onChange={onFormChange} options={text.forms} />
  </section>;
}
