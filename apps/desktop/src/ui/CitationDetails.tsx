import { Input, Label, Text, TextField } from "react-aria-components";
import { useEffect, useRef, useState, type KeyboardEvent, type RefObject } from "react";
import type { Locator, LocatorTerm, ReferenceDetails, ReferenceForm } from "../bindings";
import { composing } from "../composition";
import { INTERFACE_LOCALE, say, type MessageId } from "../i18n";
import { Icon } from "../icons";
import { Button } from "./Button";
import { ToolbarMenu } from "./ToolbarMenu";
import "./CitationDetails.css";

/** Each form by its one name; the engine writes its example. */
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
 * built: how it shows, a pop-up of the forms by name, each with the real output
 * it would paste; its page, with the type inside the field; and text before and
 * after, shown when asked for. Only the engine says a form cannot be printed:
 * those are left out of the menu, and its note says why once. */
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
  const formButton = useRef<HTMLButtonElement>(null);
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
  // Before the engine answers, every form can be tried.
  const offered = FORMS.filter((option) => !answer || choice(option.value) && !choice(option.value)?.reason);
  // Each reason once, with the forms it leaves out.
  const reasons = new Map<string, string[]>();
  for (const option of FORMS) { const reason = choice(option.value)?.reason; if (reason) reasons.set(reason, [...reasons.get(reason) ?? [], option.label]); }
  const names = new Intl.ListFormat(INTERFACE_LOCALE, { type: "conjunction" });
  const note = choices === "failed" ? say("citation-details-examples-unavailable")
    : [...reasons].map(([reason, forms]) => `${names.format(forms)}: ${reason}`).join(" · ") || undefined;
  return <div className="citation-details">
    <div className="citation-field">
      <span className="citation-label" aria-hidden="true">{say("citation-details-form")}</span>
      {offered.length ? <ToolbarMenu buttonRef={formButton} returnFocusRef={formButton} placement="bottom start" label={say("citation-details-form")}
        value={FORMS.find((option) => option.value === current)?.label ?? say("citation-details-none")} selectedKey={current ?? ""} note={note}
        sections={[{ id: "forms", items: offered.map((option) => ({ id: option.value, label: option.label, sample: choice(option.value)?.example ?? undefined, prose: true })) }]}
        onAction={(key) => { const chosen = offered.find((option) => option.value === key); if (chosen) onFormChange(chosen.value); }} />
        : <p className="citation-note">{[...reasons.keys()][0] ?? say("citation-details-none")}</p>}
    </div>
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
      {!shown.before && <Button variant="quiet" size="small" className="citation-reveal" icon={<Icon name="plus" />} onPress={() => setShown((old) => ({ ...old, before: true }))}>{say("citation-details-add-prefix")}</Button>}
      {!shown.after && <Button variant="quiet" size="small" className="citation-reveal" icon={<Icon name="plus" />} onPress={() => setShown((old) => ({ ...old, after: true }))}>{say("citation-details-add-suffix")}</Button>}
    </div>}
  </div>;
}
