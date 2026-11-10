import { useCallback, useEffect, useLayoutEffect, useRef, useState, type Ref, type RefObject } from "react";
import { Reader } from "../reader/Reader";
import { usePageSource } from "../reader/source";
import { citationDetails, isCancelled } from "../../api";
import type { FilePreview, ReferenceDetails } from "../../bindings";
import { composing } from "../../composition";
import { Icon } from "../../icons";
import { say } from "../../i18n";
import { Button } from "../../ui/Button";
import { CitationDetails } from "../../ui/CitationDetails";
import { Inspector } from "../../ui/Inspector";
import { Popover } from "../../ui/Popover";
import { Switch } from "../../ui/Switch";
import type { CitationDetailsChange, CitationReference } from "./choice";
import type { InspectorPanel } from "./useCitationInspector";

/** `entry` explains output that depends on the whole document: its short name and its sentence. */
export type CitationNotes = { entry: { short: string; text: string } | null; output: readonly string[]; closest: readonly string[] };

/** The details of one reference, or the notes on its citation. The column's
 * controls never move: everything that grows with the output sits beside them. */
export function CitationInspector({ ref, id, panel, anchor, keepOrder, heading, reference, index, selected, previewReceipt, output, failure, notes, onChange, onSubmit, onRemove, onClose }: {
  ref: Ref<HTMLElement>; id: string; panel: InspectorPanel; heading: string;
  /** What the details grow from: the reference's token, or the control that opened them. */
  anchor: RefObject<Element | null>;
  /** Keep My Order, at the foot only where the style may sort. */
  keepOrder: { checked: boolean; onChange: (on: boolean) => void } | null;
  reference: CitationReference | undefined; index: number; selected: boolean; previewReceipt?: FilePreview;
  /** Which output the citation is in; a page edit leaves it the same. */
  output: string;
  failure: string | null; notes: CitationNotes;
  onChange?: (reference: CitationReference, change: CitationDetailsChange) => void;
  onSubmit?: () => void;
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
  // One stable ref, so the shelf's own keeps its cleanup, which returns focus to the token.
  const body = useRef<HTMLElement | null>(null);
  const attach = useCallback((node: HTMLElement | null) => {
    body.current = node;
    if (typeof ref === "function") return ref(node);
    if (ref) ref.current = node;
  }, [ref]);
  // The highlighted result stays in view above the details when the list can scroll it there.
  useLayoutEffect(() => {
    const popover = body.current?.closest(".ui-popover")?.getBoundingClientRect();
    const row = document.querySelector<HTMLElement>('#results [role="option"][aria-selected="true"]');
    const list = row?.closest<HTMLElement>(".scroll-area, [data-scroll]") ?? row?.parentElement?.parentElement;
    if (!popover || !row || !list) return;
    const overlap = row.getBoundingClientRect().bottom - popover.top;
    const room = row.getBoundingClientRect().top - list.getBoundingClientRect().top;
    if (overlap > 0 && room >= overlap) list.scrollTop += overlap;
  }, []);
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
  const fields = reference && onChange && <CitationDetails autoFocus locator={reference.locator ?? null} form={form} choices={choices}
    choosePage={pdf && !reading ? () => setReading(true) : undefined}
    onLocatorChange={(locator) => onChange(reference, { locator: locator && { ...locator, input: "editor" } })}
    onFormChange={(form) => onChange(reference, { form })}
    prefix={reference.prefix ?? ""} suffix={reference.suffix ?? ""}
    onPrefixChange={(prefix) => onChange(reference, { prefix })} onSuffixChange={(suffix) => onChange(reference, { suffix })} onSubmit={onSubmit} />;
  const notesShown = [...notes.entry ? [notes.entry.text] : [], ...notes.output, ...notes.closest].map((note, i) => <p key={i} className="ui-popover-note">{note}</p>);
  const removeKeys = (event: React.KeyboardEvent<HTMLElement>) => {
    const target = event.target as HTMLElement;
    if ((event.key !== "Backspace" && event.key !== "Delete") || composing(event) || target.matches("input") || !reference || !selected) return;
    event.preventDefault(); onRemove(reference);
  };
  // The Reader keeps the side panel: a source's pages need its room.
  if (reading && pdf && reference && onChange) {
    return <Inspector ref={ref} id={id} label={say("citation-details-details", { reference: reference.title || reference.key })}
      heading={reference.title || heading} byline={byline || undefined} closeLabel={say("key-close")} onClose={onClose} reading
      footer={<div className="inspector-actions"><hr /><Button variant="quiet" className="inspector-action" aria-expanded onPress={() => setReading(false)}><span>{say("reader-hide")}</span></Button></div>}
      document={<Reader key={pdf.id} source={pdf} title={reference.title || reference.key} page={reference.locator?.label === "page" ? reference.locator.value : ""}
        onPageChange={(value) => onChange(reference, { locator: value ? { label: "page", value, input: "editor" } : null })} onClose={() => setReading(false)} />}
      onKeyDown={removeKeys}>{fields}</Inspector>;
  }
  return <Popover triggerRef={anchor} label={reference ? say("citation-details-details", { reference: reference.title || reference.key }) : say("picker-citation-details")}
    className="citation-popover">
    <section ref={attach} id={id} className="ui-popover-body" onKeyDown={removeKeys}>
      <header className="ui-popover-heading">
        <h2><bdi dir="auto">{reference ? reference.title || heading : heading}</bdi></h2>
        {byline && <p><bdi dir="auto">{byline}</bdi></p>}
      </header>
      {fields}
      {problem}
      {notesShown}
      {keepOrder && <label className="ui-popover-foot"><span>{say("picker-keep-my-order")}</span>
        <Switch aria-label={say("picker-keep-my-order")} isSelected={keepOrder.checked} onChange={keepOrder.onChange} /></label>}
    </section>
  </Popover>;
}
