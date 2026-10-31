import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type RefObject } from "react";
import { citationDetails, type SearchResult } from "../../api";
import type { FilePreview, ReferenceDetails } from "../../bindings";
import type { FormattedCitation, StyleId } from "../../format/styles";
import { copiesEntry, STYLE_NAMES } from "../../format/styles";
import { render, renderRuns } from "../../format/markup";
import { citationDetailsText, pickerText } from "../../messages";
import { choiceKey, idKey, type CitationDetailsChange, type CitationReference } from "./choice";
import { closestMatchWarnings } from "./rows";
import type { ShelfCopied } from "./session";
import { composing } from "../../composition";
import { Button } from "../../ui/Button";
import { CitationDetails } from "../../ui/CitationDetails";
import { DetailsMenu } from "../../ui/DetailsMenu";
import { keyLabel } from "../../keys";
import { currentPlatform } from "../../platform";
import { beak } from "../../brand";
import "./CitationShelf.css";

type Panel = { kind: "references" | "reference" | "failure" | "warnings"; anchor: HTMLElement; presentation: number | undefined; reference?: string; receipt?: ShelfCopied | null };

/** A short name for the menu, independent of its citation key. */
export function referenceLabel(r: SearchResult): string {
  return [r.first_author, r.year ?? ""].join(" ").trim() || r.key;
}
export function referenceLabels(records: SearchResult[], libraryName?: (r: SearchResult) => string | null): string[] {
  const labels = records.map(referenceLabel);
  const keyed = labels.map((label, i) => labels.indexOf(label) !== labels.lastIndexOf(label) && label !== records[i]!.key ? `${label} · ${records[i]!.key}` : label);
  const scoped = keyed.map((label, i) => keyed.indexOf(label) !== keyed.lastIndexOf(label)
    ? `${label} · ${libraryName?.(records[i]!) || records[i]!.title || i + 1}` : label);
  return scoped.map((label, i) => scoped.indexOf(label) !== scoped.lastIndexOf(label) ? `${label} · ${i + 1}` : label);
}

function Beak({ closing = false }: { closing?: boolean }) {
  return <svg className={`shelf-mark shelf-mark-${closing ? "close" : "open"}`} viewBox={beak.viewBox} fill="currentColor" aria-hidden="true" focusable="false"
    dangerouslySetInnerHTML={{ __html: beak.body }} />;
}

/** One line of exact engine output. All editing stays in an anchored menu;
 * a folded citation retains its full text and every reference's identity. */
export function CitationShelf({ announce, selected, highlightedReference, highlightedReferences, onChangeReference, removeLabel, onRemove, referenceLibrary, style, keys, shown, previewReceipt, outputWarnings, problem, focusable, fallbackFocusRef, presentation, present = false, copied: copiedReceipt }: {
  presentation?: number;
  present?: boolean;
  announce: boolean;
  selected: Map<string, CitationReference>;
  highlightedReference?: CitationReference;
  highlightedReferences?: CitationReference[];
  previewReceipt?: FilePreview;
  onChangeReference?: (reference: CitationReference, change: CitationDetailsChange) => void;
  removeLabel: (r: SearchResult) => string;
  onRemove: (r: SearchResult) => void;
  referenceLibrary?: (r: SearchResult) => string | null;
  style: StyleId | null;
  keys: boolean;
  shown: FormattedCitation | "failed" | "stale" | null | undefined;
  outputWarnings: readonly string[];
  problem?: string;
  focusable: boolean;
  fallbackFocusRef?: RefObject<HTMLElement | null>;
  /** The receipt belongs to this interaction, never to an older preview. */
  copied?: ShelfCopied | null;
}) {
  const copied = copiedReceipt && shown && typeof shown === "object" && shown.text === copiedReceipt.text ? copiedReceipt : null;
  const preserveFocus = useCallback((node: HTMLElement | null) => {
    if (!node) return;
    return () => {
      if (node.contains(document.activeElement) && fallbackFocusRef?.current?.isConnected) fallbackFocusRef.current.focus({ preventScroll: true });
    };
  }, [fallbackFocusRef]);
  const records = [...selected.values()];
  const references = records.length ? records : highlightedReferences ?? (highlightedReference ? [highlightedReference] : []);
  const labels = referenceLabels(references, referenceLibrary);
  const [panel, setPanel] = useState<Panel | null>(null);
  const [formChoices, setFormChoices] = useState<{ receipt: FilePreview; item: number; value: ReferenceDetails | "failed" } | null>(null);
  const menuId = useId();
  const citationId = useId();
  const page = useRef<HTMLInputElement>(null);
  const output = useRef<HTMLElement>(null);
  const count = useRef<HTMLButtonElement>(null);
  const wasVisible = useRef(shown !== undefined);
  useLayoutEffect(() => {
    if (shown === undefined && wasVisible.current && output.current?.contains(document.activeElement)) fallbackFocusRef?.current?.focus({ preventScroll: true });
    wasVisible.current = shown !== undefined;
  }, [shown, fallbackFocusRef]);
  const [compact, setCompact] = useState(false);
  useLayoutEffect(() => {
    const fit = () => {
      const size = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
      setCompact(innerWidth <= 30 * size || innerHeight <= 24 * size);
    };
    fit();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(fit);
    observer?.observe(document.documentElement);
    window.addEventListener("resize", fit);
    return () => { observer?.disconnect(); window.removeEventListener("resize", fit); };
  }, []);
  const edited = panel?.reference ? references.find((r) => idKey(r.id) === panel.reference) : references[0];
  const editedIndex = edited ? references.indexOf(edited) : -1;
  const detailsOpen = panel?.kind === "reference" || panel?.kind === "failure";
  const panelPresentation = panel?.presentation;
  useEffect(() => {
    if (!detailsOpen || panelPresentation !== presentation || !previewReceipt || editedIndex < 0) return;
    let live = true;
    void citationDetails(previewReceipt.ticket, previewReceipt.content_hash, editedIndex).then(
      (value) => { if (live) setFormChoices({ receipt: previewReceipt, item: editedIndex, value }); },
      () => { if (live) setFormChoices({ receipt: previewReceipt, item: editedIndex, value: "failed" }); },
    );
    return () => { live = false; };
  }, [detailsOpen, previewReceipt, editedIndex, presentation, panelPresentation]);
  const choices = formChoices?.receipt === previewReceipt && formChoices?.item === editedIndex ? formChoices.value : shown === "failed" && !previewReceipt ? "refused" : undefined;
  useLayoutEffect(() => {
    if (!panel) return;
    if (panel.presentation !== presentation || panel.receipt !== copied || panel.kind === "reference" && !edited || panel.kind === "references" && references.length === 0 || copied && panel.kind !== "warnings") {
      if (document.getElementById(menuId)?.contains(document.activeElement)) fallbackFocusRef?.current?.focus({ preventScroll: true });
      setPanel(null); return;
    }
    if (panel.anchor.isConnected && !panel.anchor.closest("[hidden]")) return;
    const anchor = count.current ?? output.current;
    if (anchor) setPanel({ ...panel, anchor }); else setPanel(null);
  });
  const partialWarnings = copied ? [...copied.closestMatches ?? []] : closestMatchWarnings(references);
  const currentWarnings = copied ? copied.warnings.slice(0, copied.warnings.length - partialWarnings.length) : outputWarnings;
  const warnings = [...currentWarnings, ...partialWarnings];
  const entryNotice = style && copiesEntry(style) ? pickerText.entryOnly(STYLE_NAMES[style]) : null;
  const statusLabel = [warnings.length > 0 && pickerText.warningCount(warnings.length), entryNotice && pickerText.entryOnlyShort,
    partialWarnings.length > 0 && pickerText.closestMatches].filter(Boolean).join(" · ");
  const open = (kind: Panel["kind"], anchor: HTMLElement, reference?: CitationReference) => setPanel({ kind, anchor, presentation, receipt: copied, reference: reference && idKey(reference.id) });
  const edit = (reference: CitationReference, trigger: HTMLElement, fromList = false) => {
    open("reference", fromList && panel ? panel.anchor : trigger, reference);
  };
  const remove = (reference: CitationReference) => { setPanel(null); onRemove(reference); };
  const isSelected = (reference: CitationReference) => records.some((r) => idKey(r.id) === idKey(reference.id));
  const fullProblem = problem ?? (style ? pickerText.previewStyleError(STYLE_NAMES[style]) : pickerText.previewError);
  const ready = shown && typeof shown === "object" ? shown : null;
  const referenceIds = JSON.stringify(references.map((reference) => idKey(reference.id)));
  const retained = useRef<{ presentation: typeof presentation; ids: string; value: FormattedCitation } | null>(null);
  useLayoutEffect(() => { if (ready) retained.current = { presentation, ids: referenceIds, value: ready }; }, [ready, presentation, referenceIds]);
  const meaning = choiceKey(references);
  const pendingOwner = useMemo(() => ({ meaning, style, keys, presentation }), [meaning, style, keys, presentation]);
  const [expired, setExpired] = useState<object | null>(null);
  useEffect(() => {
    if (shown !== null) { setExpired(null); return; }
    const timer = window.setTimeout(() => setExpired(pendingOwner), 300);
    return () => window.clearTimeout(timer);
  }, [shown, pendingOwner]);
  const previous = shown === null && expired !== pendingOwner && retained.current?.presentation === presentation ? retained.current?.value ?? null : null;
  const retainedSegments = Boolean(previous && retained.current?.ids === referenceIds);
  const visible = ready ?? (retainedSegments ? previous : null);
  const text = visible?.text;
  const single = references.length === 1 ? references[0] : undefined;
  const hasShelf = present || copied || references.length || shown !== undefined;
  const fullCitation = ready && <section aria-label="Full citation" className="shelf-full-citation" tabIndex={0}><bdi dir="auto">{ready.markup.length ? render(ready.markup) : ready.text}</bdi></section>;
  const detailsTitle = panel?.kind === "warnings" ? pickerText.citationWarnings : panel?.kind === "references" ? pickerText.selectedReferences : edited
    ? citationDetailsText.details(edited.title || edited.key) : pickerText.citationDetails;
  const segmentKeys = (event: KeyboardEvent, reference: CitationReference) => {
    if (composing(event)) return;
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      event.preventDefault(); event.stopPropagation();
      const segments = [...output.current?.querySelectorAll<HTMLButtonElement>(".shelf-segment-button") ?? []];
      const step = (event.key === "ArrowRight" ? 1 : -1) * (getComputedStyle(event.currentTarget).direction === "rtl" ? -1 : 1);
      segments[Math.min(segments.length - 1, Math.max(0, segments.indexOf(event.currentTarget as HTMLButtonElement) + step))]?.focus();
    }
    if ((event.key === "Backspace" || event.key === "Delete") && isSelected(reference)) {
      event.preventDefault(); event.stopPropagation(); remove(reference);
    }
  };
  const keyText = (value: string) => keys ? value.split(/([^\p{L}\p{N}\s]+)/u).map((part, index) => index % 2 ? <span className="shelf-punctuation" key={index}>{part}</span> : part) : value;
  const segments = visible?.spans ?? (shown === undefined || shown === null && !previous ? references.map((_, item) => ({ item, text: `${item ? "; " : ""}${labels[item]}` })) : undefined);
  const manySegments = !single && !copied && segments?.map((span, index) => {
    const reference = span.item === null ? undefined : references[span.item];
    const contents = !keys && "runs" in span && span.runs ? renderRuns(span.runs) : keyText(span.text);
    if (!reference || !onChangeReference || copied) return <span key={index} className="shelf-shared">{contents}</span>;
    const name = labels[span.item!]!;
    return <span key={idKey(reference.id)} className="shelf-segment" data-reference-key={reference.key} data-built={isSelected(reference) || undefined}
      data-menu-open={panel?.kind === "reference" && panel.reference === idKey(reference.id) || undefined}>
      <Button variant="quiet" size="small" className="shelf-segment-button" aria-label={citationDetailsText.edit(name)}
        aria-describedby={`${citationId}-${index}`} aria-keyshortcuts={isSelected(reference) ? "Backspace Delete" : undefined}
        aria-haspopup="dialog" aria-expanded={panel?.kind === "reference" && panel.reference === idKey(reference.id)}
        onKeyDown={(event) => segmentKeys(event, reference)} onPress={(event) => edit(reference, event.target as HTMLElement)}>
        <bdi id={`${citationId}-${index}`} dir="auto" className="preview-text" title={span.text}>{contents}</bdi>
      </Button>
      {isSelected(reference) && <Button variant="icon" size="small" className="shelf-remove" glyph="close" excludeFromTabOrder aria-label={removeLabel(reference)} onPress={() => remove(reference)} />}
    </span>;
  });
  const singleSegment = single && onChangeReference && !copied ? <span className="shelf-segment" data-reference-key={single.key} data-built={isSelected(single) || undefined} data-menu-open={panel?.kind === "reference" || undefined}>
            <Button variant="quiet" size="small" className="shelf-segment-button" aria-label={citationDetailsText.edit(labels[0]!)}
              aria-keyshortcuts={isSelected(single) ? "Backspace Delete" : undefined}
              aria-describedby={text !== undefined ? citationId : undefined}
              aria-haspopup="dialog" aria-expanded={panel?.kind === "reference"} aria-controls={panel?.kind === "reference" ? menuId : undefined}
              onKeyDown={(event) => segmentKeys(event, single)} onPress={(event) => edit(single, event.target as HTMLElement)}>
              <bdi id={text !== undefined ? citationId : undefined} dir="auto" className={text !== undefined ? "preview-text" : "shelf-reference-name"} title={text ?? single.title}>{text !== undefined ? keys ? keyText(text) : visible && visible.markup.length ? render(visible.markup) : text : labels[0]}</bdi>
            </Button>
            {isSelected(single) && <Button variant="icon" size="small" className="shelf-remove" glyph="close" excludeFromTabOrder
              aria-label={removeLabel(single)} onPress={() => remove(single)} />}
          </span> : null;
  return <>
    <p className="visually-hidden" role="status" aria-label={pickerText.citationSelection} aria-live="polite" aria-atomic="true">
      {announce ? [pickerText.selectionCount(selected.size), ...partialWarnings].join(" ") : ""}
    </p>
    {hasShelf ? <div className="shelf" data-folded={compact || undefined} data-copied={copied ? "" : undefined} ref={preserveFocus}>
      <section ref={(node) => { output.current = node; }} aria-label={shown !== undefined || copied ? pickerText.preview : pickerText.citationDetails}
        tabIndex={focusable && shown !== undefined && !copied ? 0 : undefined}
        className="shelf-output" data-keys={keys || undefined} data-state={!copied && (shown === null || typeof shown === "string") ? shown ?? "pending" : undefined}>
        <span className="shelf-leading"><Beak /></span>
        <div className="shelf-citation" role={selected.size ? "group" : undefined} aria-label={selected.size ? pickerText.selectedReferences : undefined}>
          {previous && !retainedSegments ? <bdi className="preview-text shelf-retained" dir="auto" title={previous.text}>{keyText(previous.text)}</bdi> : shown !== "failed" && shown !== "stale" && singleSegment}
          {text !== undefined ? singleSegment ? null : manySegments ? <span className="shelf-fragments" dir={keys ? "ltr" : "auto"} title={text}>{manySegments}</span> : <bdi id={citationId} dir="auto" className="preview-text" title={text}>
            {ready?.markup.length && !keys ? render(ready.markup) : keyText(text)}
          </bdi> : shown === "failed" ? <><span className="shelf-state-icon" data-state="failed" aria-hidden="true">!</span><p title={fullProblem}>{fullProblem}</p></> : shown === "stale" ? <><span className="shelf-state-icon" data-state="stale" aria-hidden="true">!</span><p role="status">{pickerText.shelfStale}</p></>
            : shown === undefined && manySegments ? <span className="shelf-fragments">{manySegments}</span> : shown === null && !previous ? <p role="status">{pickerText.previewPending}</p> : null}
          {copied && <Beak closing />}
        </div>
        <div className="shelf-actions">
          {copied && <span role="status" aria-label={pickerText.copied} className="shelf-copied"><svg viewBox="0 0 20 20" aria-hidden="true"><path d="m5 10.5 3.2 3.2L15 6.5" pathLength="1" /></svg>{pickerText.copied}</span>}
          {!copied && references.length > 1 && <Button ref={count} variant="quiet" size="small" className="shelf-count" aria-haspopup="dialog"
            aria-label={pickerText.allChosen(references.length)} title={pickerText.allChosen(references.length)} aria-expanded={panel?.kind === "references" || panel?.anchor === count.current}
            aria-controls={panel?.kind === "references" || panel?.anchor === count.current ? menuId : undefined}
            onPress={(event) => open("references", event.target as HTMLElement)}>{references.length}</Button>}
          {statusLabel && <Button variant="quiet" size="small" className="shelf-warnings" aria-label={statusLabel} title={statusLabel}
            aria-haspopup="dialog" aria-expanded={panel?.kind === "warnings"} aria-controls={panel?.kind === "warnings" ? menuId : undefined}
            onPress={(event) => open("warnings", event.target as HTMLElement)}>{compact ? warnings.length > 0 ? `! ${warnings.length}` : "i" : <>
              {warnings.length > 0 && <span>{pickerText.warningCount(warnings.length)}</span>}
              {entryNotice && <span>{pickerText.entryOnlyShort}</span>}
            </>}</Button>}
          {!copied && (shown === "failed" || shown === "stale") && <Button variant="quiet" size="small" aria-haspopup="dialog" aria-expanded={panel?.kind === "failure"}
            aria-controls={panel?.kind === "failure" ? menuId : undefined} onPress={(event) => open("failure", output.current ?? event.target as HTMLElement)}>{pickerText.details}</Button>}
        </div>
      </section>
    </div> : null}
    {panel && <DetailsMenu id={menuId} anchor={panel.anchor} returnFocusRef={fallbackFocusRef} title={detailsTitle}
      onRemove={panel.kind === "reference" && edited && isSelected(edited) ? () => remove(edited) : undefined}
      heading={panel.kind === "references" || panel.kind === "warnings" ? detailsTitle : labels[editedIndex] ?? detailsTitle} onClose={() => setPanel(null)}>
      {panel.kind === "references" ? <>
        <ul className="shelf-details-list" aria-label={pickerText.selectedReferences}>{references.map((reference, i) => <li key={idKey(reference.id)} data-reference-key={reference.key}>
        <Button variant="quiet" aria-label={citationDetailsText.edit(labels[i]!)} aria-describedby={`${menuId}-reference-${i}`} onKeyDown={(event) => segmentKeys(event, reference)} onPress={(event) => edit(reference, event.target as HTMLElement, true)}>
          <span className="shelf-reference-summary"><bdi>{labels[i]}</bdi><bdi className="shelf-reference-title" id={`${menuId}-reference-${i}`}>{reference.title}</bdi></span>
          {reference.locator && <bdi className="shelf-detail-locator">{reference.locator.label} {reference.locator.value}</bdi>}
          {reference.partial && <span className="shelf-pill">{pickerText.closestMatch}</span>}
        </Button>
      </li>)}</ul>{fullCitation}</> : panel.kind === "warnings" ? <>
        {entryNotice && <><h3>{pickerText.entryOnlyShort}</h3><p>{entryNotice}</p></>}
        {currentWarnings.map((warning, i) => <p key={`output-${i}`}>{warning}</p>)}
        {partialWarnings.length > 0 && <h3>{pickerText.closestMatches}</h3>}
        {partialWarnings.map((warning, i) => <p key={`partial-${i}`}>{warning}</p>)}
      </> : <>
        {edited && <p className="shelf-detail-title">{edited.title || edited.key}</p>}
        {shown === "failed" && <p role="alert" className="shelf-problem">{fullProblem}</p>}
        {edited && onChangeReference && <CitationDetails key={idKey(edited.id)} pageRef={page} autoFocus label={citationDetailsText.details(edited.title || edited.key)}
          page={edited.locator?.label === "page" ? edited.locator.value : ""} choices={choices}
          onPageChange={(value) => onChangeReference(edited, { locator: value.trim() ? { label: "page", value, input: "page-editor" } : null })}
          onFormChange={(form) => onChangeReference(edited, { form })} />}
        {edited && <Button variant="quiet" className="shelf-menu-remove" aria-keyshortcuts="Backspace Delete" isDisabled={!isSelected(edited)} onPress={() => remove(edited)}>
          <span>Remove from citation</span><span aria-hidden="true">{keyLabel("Backspace", currentPlatform() === "macos")}</span>
        </Button>}
        {fullCitation}
        {warnings.map((warning, i) => <p key={i}>{warning}</p>)}
      </>}
    </DetailsMenu>}
  </>;
}
