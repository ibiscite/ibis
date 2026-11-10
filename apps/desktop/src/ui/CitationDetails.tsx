import { Input, Label, Text, TextField } from "react-aria-components";
import { useEffect, useRef, useState, type KeyboardEvent, type RefObject } from "react";
import type { Locator, LocatorTerm, ReferenceDetails, ReferenceForm } from "../bindings";
import { composing } from "../composition";
import { say, type MessageId } from "../i18n";
import { Button } from "./Button";
import { Segmented } from "./Segmented";
import { ToolbarMenu } from "./ToolbarMenu";
import "./CitationDetails.css";

/** Each form by its one name, for before the engine has written examples. */
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
const termShort = (term: LocatorTerm) => say(`locator-short-${term}` as MessageId);

/** One citation's details for one reference, the same wherever a citation is
 * built: how it shows, as the real outputs to choose from; its page, with the
 * type inside the field; and text before and after, shown when asked for. Only
 * the engine says a form cannot be printed, and only those it can are offered. */
export function CitationDetails({ locator, form, prefix, suffix, locatorRef, choices, autoFocus = false, choosePage, onLocatorChange, onFormChange, onPrefixChange, onSuffixChange, onSubmit }: {
  /** The locator as typed, or captured from search; null when there is none. */
  locator: Locator | null; form: ReferenceForm; prefix: string; suffix: string;
  locatorRef?: RefObject<HTMLInputElement | null>;
  autoFocus?: boolean;
  choices?: ReferenceDetails | "failed";
  /** Picks the page from the reference's PDF, where it has one. */
  choosePage?: () => void;
  /** Null when the field is cleared; the type stays until another is chosen. */
  onLocatorChange: (value: Locator | null) => void;
  onFormChange: (value: ReferenceForm) => void;
  onPrefixChange: (value: string) => void;
  onSuffixChange: (value: string) => void;
  /** Enter in a text field does what it does in Search. */
  onSubmit?: () => void;
}) {
  const typeButton = useRef<HTMLButtonElement>(null);
  // A pick or Esc in the type menu returns to the value, where typing continues.
  const ownField = useRef<HTMLInputElement>(null);
  const field = locatorRef ?? ownField;
  // A cleared field keeps its type, so a new value never changes what it means.
  const [term, setTerm] = useState<LocatorTerm>(locator?.label ?? "page");
  useEffect(() => { if (locator) setTerm(locator.label); }, [locator]);
  const [shown, setShown] = useState({ before: prefix !== "", after: suffix !== "" });
  const submit = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter" && !event.repeat && !composing(event)) { event.preventDefault(); onSubmit?.(); }
  };
  const answer = typeof choices === "object" ? choices : undefined;
  // The engine answers null where no name form applies, as for Keys only.
  const current = answer ? answer.current_form : form;
  const choice = (value: ReferenceForm) => answer?.forms.find((option) => option.form === value);
  // Only the forms this output can print, each as the text it would paste.
  const offered = FORMS.filter((option) => !choice(option.value)?.reason)
    .map((option) => ({ value: option.value, label: choice(option.value)?.example ?? option.label }));
  const hidden = FORMS.filter((option) => choice(option.value)?.reason);
  const reasons = new Set(hidden.map((option) => choice(option.value)?.reason));
  return <div className="citation-details">
    <div className="citation-field">
      <span className="citation-label" aria-hidden="true">{say("citation-details-form")}</span>
      {offered.length > 1 ? <Segmented label={say("citation-details-form")} value={current} options={offered} onChange={onFormChange} />
        : <p className="citation-note">{offered.length ? offered[0]!.label : [...reasons][0] ?? say("citation-details-none")}</p>}
    </div>
    {/* What the others are, quietly, where they are looked for. */}
    {offered.length > 1 && hidden.length > 0 && <p className="citation-note citation-field-note">
      {hidden.map((option) => `${option.label}: ${choice(option.value)?.reason}`).join(" · ")}</p>}
    {choices === "failed" && <p className="citation-note citation-field-note">{say("citation-details-examples-unavailable")}</p>}
    <div className="citation-field">
      <span className="citation-label" aria-hidden="true">{say("citation-details-page")}</span>
      <div className="citation-locator">
        <ToolbarMenu buttonRef={typeButton} returnFocusRef={field} placement="bottom start" label={say("citation-details-locator-type")} value={termName(term)} selectedKey={term}
          filterLabel={say("citation-details-locator-filter")} button={{ className: "citation-locator-type", label: `${say("citation-details-locator-type")}: ${termName(term)}`, content: termShort(term), quiet: true }}
          sections={[{ id: "terms", items: LOCATOR_TERMS.map((option) => ({ id: option, label: termName(option), sample: termShort(option) })) }]}
          onAction={(key) => {
            const chosen = LOCATOR_TERMS.find((option) => option === key);
            if (!chosen) return;
            setTerm(chosen);
            if (locator) onLocatorChange({ ...locator, label: chosen });
          }} />
        <TextField aria-label={termName(term)} value={locator?.value ?? ""} onChange={(value) => onLocatorChange(value.trim() ? { label: term, value } : null)}>
          <Input ref={field} autoFocus={autoFocus} placeholder={term === "page" ? say("citation-details-page-placeholder") : say("citation-details-locator-placeholder")} onKeyDown={submit} />
          <Text slot="description" className="visually-hidden">{say("citation-details-page-hint")}</Text>
        </TextField>
      </div>
    </div>
    {choosePage && <Button variant="quiet" size="small" className="citation-reveal" onPress={choosePage}>{say("reader-show")}</Button>}
    {shown.before && <TextField className="citation-field" value={prefix} onChange={onPrefixChange}>
      <Label className="citation-label">{say("citation-details-prefix")}</Label>
      <Input autoFocus={!autoFocus && prefix === ""} onKeyDown={submit} />
    </TextField>}
    {shown.after && <TextField className="citation-field" value={suffix} onChange={onSuffixChange}>
      <Label className="citation-label">{say("citation-details-suffix")}</Label>
      <Input autoFocus={!autoFocus && suffix === ""} onKeyDown={submit} />
    </TextField>}
    {(!shown.before || !shown.after) && <div className="citation-reveals">
      {!shown.before && <Button variant="quiet" size="small" className="citation-reveal" onPress={() => setShown((old) => ({ ...old, before: true }))}>{say("citation-details-add-prefix")}</Button>}
      {!shown.after && <Button variant="quiet" size="small" className="citation-reveal" onPress={() => setShown((old) => ({ ...old, after: true }))}>{say("citation-details-add-suffix")}</Button>}
    </div>}
  </div>;
}
