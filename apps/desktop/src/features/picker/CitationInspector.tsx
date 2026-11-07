import { useEffect, useState, type Ref } from "react";
import { Reader } from "../reader/Reader";
import { usePageSource } from "../reader/source";
import { citationDetails, isCancelled } from "../../api";
import type { FilePreview, ReferenceDetails } from "../../bindings";
import { composing } from "../../composition";
import type { FormattedCitation } from "../../format/outputs";
import { render } from "../../format/markup";
import { Icon } from "../../icons";
import { keyLabel } from "../../keys";
import { say } from "../../i18n";
import { currentPlatform } from "../../platform";
import { Button } from "../../ui/Button";
import { CitationDetails } from "../../ui/CitationDetails";
import { Inspector } from "../../ui/Inspector";
import type { SourceActionProps } from "../../ui/SourceAction";
import type { CitationDetailsChange, CitationReference } from "./choice";
import type { InspectorPanel } from "./useCitationInspector";

/** `entry` explains output that depends on the whole document: its short name and its sentence. */
export type CitationNotes = { entry: { short: string; text: string } | null; output: readonly string[]; closest: readonly string[] };

/** The details of one reference, or the notes on its citation. The column's
 * controls never move: everything that grows with the output sits beside them. */
export function CitationInspector({ ref, id, panel, heading, reference, index, selected, previewReceipt, output, citation, failure, notes, source, keys, onChange, onSubmit, onOpenSource, onRemove, onClose }: {
  ref: Ref<HTMLElement>; id: string; panel: InspectorPanel; heading: string;
  reference: CitationReference | undefined; index: number; selected: boolean; previewReceipt?: FilePreview;
  /** Which output the citation is in; a page edit leaves it the same. */
  output: string;
  citation: FormattedCitation | null; failure: string | null; notes: CitationNotes;
  source?: SourceActionProps; keys: boolean;
  onChange?: (reference: CitationReference, change: CitationDetailsChange) => void;
  onSubmit?: () => void; onOpenSource?: (reference: CitationReference) => void;
  onRemove: (reference: CitationReference) => void; onClose: () => void;
}) {
  const [details, setDetails] = useState<{ receipt: FilePreview; output: string; value: ReferenceDetails | "failed" }>();
  const editing = panel.kind !== "warnings";
  const pdf = usePageSource(editing ? reference : undefined);
  // Reading belongs to this opening of these details: closing them ends it.
  const [reading, setReading] = useState(false);
  useEffect(() => {
    if (!editing || !previewReceipt || index < 0) return;
    let live = true;
    void citationDetails(previewReceipt.ticket, previewReceipt.content_hash, index).then(
      (value) => { if (live) setDetails({ receipt: previewReceipt, output, value }); },
      (error) => { if (live && !isCancelled(error)) setDetails({ receipt: previewReceipt, output, value: "failed" }); },
    );
    return () => { live = false; };
  }, [editing, previewReceipt, index, output]);
  const problem = failure && <p role="alert" tabIndex={0} className="inspector-citation inspector-problem"><Icon name="warning" />{failure}</p>;
  if (!editing) {
    const { entry, output, closest } = notes;
    return <Inspector ref={ref} id={id} label={say("picker-citation-warnings")} heading={say("picker-citation-warnings")} closeLabel={say("key-close")} readOnly onClose={onClose}>
      {entry && <><h3>{entry.short}</h3><p>{entry.text}</p></>}
      {output.map((warning, i) => <p key={`output-${i}`}>{warning}</p>)}
      {closest.length > 0 && <h3>{say("picker-closest-matches")}</h3>}
      {closest.map((warning, i) => <p key={`partial-${i}`}>{warning}</p>)}
      {problem}
    </Inspector>;
  }
  const form = reference?.form ?? "normal";
  // Examples stay while the next arrive, so typing a page never flickers. The
  // rest of an older answer says nothing about the citation now being made,
  // unless this output has no forms at all, which a page cannot change.
  const choices = !details || details.value === "failed" || details.receipt === previewReceipt
    || details.output === output && details.value.forms.every((choice) => choice.reason) ? details?.value
    : { current_form: form, forms: details.value.forms.map((choice) => ({ ...choice, reason: null })) };
  const byline = reference ? [reference.authors, reference.container, reference.year].filter(Boolean).join(" · ") : undefined;
  return <Inspector ref={ref} id={id} label={reference ? say("citation-details-details", { reference: reference.title || reference.key }) : say("picker-citation-details")}
    heading={reference ? reference.title || heading : heading} byline={byline || undefined} closeLabel={say("key-close")} onClose={onClose}
    reading={reading && pdf !== null}
    document={reading && pdf && reference && onChange ? <Reader key={pdf.id} source={pdf} title={reference.title || reference.key} page={reference.locator?.label === "page" ? reference.locator.value : ""}
      onPageChange={(value) => onChange(reference, { locator: value ? { label: "page", value, input: "page-editor" } : null })} onClose={() => setReading(false)} /> : undefined}
    onKeyDown={(event) => {
      const target = event.target as HTMLElement;
      if ((event.key !== "Backspace" && event.key !== "Delete") || composing(event) || target.matches("input") || !reference || !selected) return;
      event.preventDefault(); onRemove(reference);
    }}>
    {reference && onChange && <CitationDetails autoFocus page={reference.locator?.label === "page" ? reference.locator.value : ""} form={form} choices={choices}
      onPageChange={(value) => onChange(reference, { locator: value.trim() ? { label: "page", value, input: "page-editor" } : null })}
      onFormChange={(form) => onChange(reference, { form })} onSubmit={onSubmit} />}
    {/* What Enter will paste, in full, right under the fields that change it. */}
    {citation && <section aria-label={say("picker-full-citation")} tabIndex={0} className="inspector-citation" data-keys={keys || undefined}><bdi dir="auto">{citation.markup.length && !keys ? render(citation.markup) : citation.text}</bdi></section>}
    {problem}
    {[...notes.entry ? [notes.entry.text] : [], ...notes.output, ...notes.closest].map((note, i) => <p key={i} className="inspector-citation">{note}</p>)}
    {reference && <hr />}
    {reference && pdf && onChange && <Button variant="quiet" className="inspector-action" aria-expanded={reading} onPress={() => setReading(!reading)}>
      <span>{reading ? say("reader-hide") : say("reader-show")}</span>
    </Button>}
    {reference && source && <Button variant="quiet" className="inspector-action" isDisabled={source.busy} onPress={() => { onClose(); onOpenSource?.(reference); }}>
      <span>{source.label}</span>{source.keys && <span className="inspector-keys" aria-hidden="true">{keyLabel(source.keys, currentPlatform() === "macos")}</span>}
    </Button>}
    {reference && <Button variant="quiet" className="inspector-action" aria-keyshortcuts="Backspace Delete" isDisabled={!selected} onPress={() => onRemove(reference)}>
      <span>{say("key-remove-from-citation")}</span><span className="inspector-keys" aria-hidden="true">{keyLabel("Backspace", currentPlatform() === "macos")}</span>
    </Button>}
  </Inspector>;
}
