import { Input, Label, Text, TextField } from "react-aria-components";
import { useRef, useState, type KeyboardEvent, type RefObject } from "react";
import type { Locator, LocatorTerm, ReferenceDetails, ReferenceForm } from "../bindings";
import { composing } from "../composition";
import { say, type MessageId } from "../i18n";
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

/** The 29 CSL locator types, Page first, as Zotero's dialog lists them. */
const LOCATOR_TERMS: readonly LocatorTerm[] = ["page", "chapter", "section", "paragraph", "line", "verse", "volume", "book", "part", "figure", "table", "note",
  "appendix", "article-locator", "canon", "column", "elocation", "equation", "folio", "issue", "opus", "rule", "scene", "sub-verbo", "supplement", "timestamp",
  "title-locator", "version", "act"];
const termName = (term: LocatorTerm) => say(`locator-${term}` as MessageId);

/** The same per-reference fields wherever a citation is being prepared. Only the
 * engine says a form cannot be used: until it answers, every form can be tried,
 * so a refused citation can always be corrected from here. */
export function CitationDetails({ locator, form, prefix, suffix, locatorRef, choices, autoFocus = false, onLocatorChange, onFormChange, onPrefixChange, onSuffixChange, onSubmit }: {
  /** The locator as typed, or captured from search; null when there is none. */
  locator: Locator | null; form: ReferenceForm; prefix: string; suffix: string;
  locatorRef?: RefObject<HTMLInputElement | null>;
  autoFocus?: boolean;
  choices?: ReferenceDetails | "failed";
  /** Null when the field is cleared; the type stays the pop-up's until a value is typed. */
  onLocatorChange: (value: Locator | null) => void;
  onFormChange: (value: ReferenceForm) => void;
  onPrefixChange: (value: string) => void;
  onSuffixChange: (value: string) => void;
  /** Enter in a text field does what it does in Search. */
  onSubmit?: () => void;
}) {
  const typeButton = useRef<HTMLButtonElement>(null);
  // An empty field keeps the type chosen for it until a value is typed.
  const [emptyTerm, setEmptyTerm] = useState<LocatorTerm>("page");
  const term = locator?.label ?? emptyTerm;
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
    <div className="citation-field">
      <ToolbarMenu buttonRef={typeButton} returnFocusRef={locatorRef} placement="bottom start" label={say("citation-details-locator-type")} value={termName(term)} selectedKey={term}
        filterLabel={say("citation-details-locator-filter")} button={{ className: "citation-locator-type", label: say("citation-details-locator-type"), content: termName(term), quiet: true }}
        sections={[{ id: "terms", items: LOCATOR_TERMS.map((option) => ({ id: option, label: termName(option) })) }]}
        onAction={(key) => {
          const chosen = LOCATOR_TERMS.find((option) => option === key);
          if (!chosen) return;
          setEmptyTerm(chosen);
          if (locator) onLocatorChange({ ...locator, label: chosen });
        }} />
      <TextField className="citation-locator-value" aria-label={termName(term)} value={locator?.value ?? ""}
        onChange={(value) => onLocatorChange(value.trim() ? { label: term, value } : null)}>
        <Input ref={locatorRef} autoFocus={autoFocus} placeholder={term === "page" ? say("citation-details-page-placeholder") : say("citation-details-locator-placeholder", { type: termName(term) })} onKeyDown={submit} />
        <Text slot="description" className="visually-hidden">{say("citation-details-page-hint")}</Text>
      </TextField>
    </div>
    <div className="citation-field citation-affixes">
      <TextField className="citation-affix" value={prefix} onChange={onPrefixChange}>
        <Label className="citation-label">{say("citation-details-prefix")}</Label>
        <Input onKeyDown={submit} />
      </TextField>
      <TextField className="citation-affix" value={suffix} onChange={onSuffixChange}>
        <Label className="citation-label">{say("citation-details-suffix")}</Label>
        <Input onKeyDown={submit} />
      </TextField>
    </div>
  </div>;
}
