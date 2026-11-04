import { Icon } from "../../icons";
import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type ComponentProps, type RefObject } from "react";
import type { SearchResult } from "../../api";
import type { FilePreview } from "../../bindings";
import type { FormattedCitation, StyleId } from "../../format/styles";
import { copiesEntry, STYLE_NAMES } from "../../format/styles";
import { render, renderRuns } from "../../format/markup";
import { citationDetailsText, pickerText } from "../../messages";
import { choiceKey, idKey, type CitationDetailsChange, type CitationReference } from "./choice";
import { closestMatchWarnings } from "./rows";
import type { ShelfCopied } from "./session";
import { composing } from "../../composition";
import { Button } from "../../ui/Button";
import { useInterfaceMessages } from "../../i18n";
import type { SourceActionProps } from "../../ui/SourceAction";
import { ShelfSegment } from "../../ui/ShelfSegment";
import { ToolbarMenu } from "../../ui/ToolbarMenu";
import { beak } from "../../brand";
import { CitationInspector } from "./CitationInspector";
import type { CitationInspectorState, InspectorPanel } from "./useCitationInspector";
import "./CitationShelf.css";

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

/** One line of exact engine output. All editing stays in the inspector above it;
 * a folded citation retains its full text and every reference's identity. */
export function CitationShelf({ inspector, onSubmit, copyProblem, source, onOpenSource, announce, selected, highlightedReference, highlightedReferences, onChangeReference, removeLabel, onRemove, referenceLibrary, style, keys, shown, previewReceipt, outputWarnings, problem, focusable, fallbackFocusRef, presentation, present = false, copied: copiedReceipt }: {
  inspector: CitationInspectorState;
  /** Enter in Page does what it does in Search. */
  onSubmit?: () => void;
  copyProblem?: string | null;
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
  /** More's first item, offered in the inspector with the same words and keys. */
  source?: SourceActionProps;
  onOpenSource?: (reference: CitationReference) => void;
}) {
  const { text: interfaceText } = useInterfaceMessages();
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
  const { panel, show, close } = inspector;
  const inspectorId = useId();
  const citationId = useId();
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
  const partialWarnings = copied ? [...copied.closestMatches ?? []] : closestMatchWarnings(references);
  const currentWarnings = copied ? copied.warnings.slice(0, copied.warnings.length - partialWarnings.length) : outputWarnings;
  const warnings = [...currentWarnings, ...partialWarnings];
  const entryNotice = style && copiesEntry(style) ? pickerText.entryOnly(STYLE_NAMES[style]) : null;
  const statusLabel = [warnings.length > 0 && pickerText.warningCount(warnings.length), entryNotice && pickerText.entryOnlyShort,
    partialWarnings.length > 0 && pickerText.closestMatches].filter(Boolean).join(" · ");
  const open = (kind: InspectorPanel["kind"], origin: HTMLElement, reference?: CitationReference) => show({ kind, origin, presentation, receipt: copied, reference: reference && idKey(reference.id) });
  const inspecting = (reference: CitationReference) => panel?.kind === "reference" && panel.reference === idKey(reference.id);
  // As with Mail's tokens, a segment selects what the details show; pressed again, it closes them.
  const edit = (reference: CitationReference, origin: HTMLElement) => { if (inspecting(reference)) close(); else open("reference", origin, reference); };
  const toggle = (kind: InspectorPanel["kind"], origin: HTMLElement, reference?: CitationReference) => { if (panel?.kind === kind) close(); else open(kind, origin, reference); };
  const remove = (reference: CitationReference) => { close(); onRemove(reference); };
  const isSelected = (reference: CitationReference) => records.some((r) => idKey(r.id) === idKey(reference.id));
  const fullProblem = problem ?? (style ? pickerText.previewStyleError(STYLE_NAMES[style]) : pickerText.previewError);
  const ready = shown && typeof shown === "object" ? shown : null;
  const referenceIds = JSON.stringify(references.map((reference) => idKey(reference.id)));
  const retained = useRef<{ presentation: typeof presentation; ids: string; value: FormattedCitation } | null>(null);
  useLayoutEffect(() => { if (ready) retained.current = { presentation, ids: referenceIds, value: ready }; }, [ready, presentation, referenceIds]);
  const meaning = choiceKey(references);
  const pendingOwner = useMemo(() => ({ meaning, style, keys, presentation }), [meaning, style, keys, presentation]);
  const [expired, setExpired] = useState<object | null>(null);
  // While the next preview is made, hold the last citation for 300 ms.
  const waiting = shown === null;
  useEffect(() => {
    if (!waiting) { setExpired(null); return; }
    const timer = window.setTimeout(() => setExpired(pendingOwner), 300);
    return () => window.clearTimeout(timer);
  }, [waiting, pendingOwner]);
  const previous = waiting && expired !== pendingOwner && retained.current?.presentation === presentation ? retained.current?.value ?? null : null;
  const retainedSegments = Boolean(previous && retained.current?.ids === referenceIds);
  const visible = ready ?? (retainedSegments ? previous : null);
  const text = visible?.text;
  const single = references.length === 1 ? references[0] : undefined;
  // The panel says these in words, so nothing in the shelf comes and goes while typing.
  const detailsOpen = panel !== null && panel.kind !== "warnings";
  const hasShelf = present || copied || references.length || shown !== undefined || previous;
  useLayoutEffect(() => {
    if (!panel) return;
    if (panel.presentation !== presentation || panel.receipt !== copied || !hasShelf || panel.reference && !edited || copied && panel.kind !== "warnings") {
      if (document.getElementById(inspectorId)?.contains(document.activeElement)) fallbackFocusRef?.current?.focus({ preventScroll: true });
      close();
    }
  });
  const opened = useRef(panel);
  if (panel) opened.current = panel;
  // Esc and the close button hand focus back to the reference. Anything else that
  // retires the inspector has already moved focus, and is left alone. Focus moves
  // after the commit: React drops focus events during one, and the ring with them.
  const heldFocus = useRef(false);
  const returnFocus = useCallback((node: HTMLElement | null) => {
    if (!node) return;
    return () => { heldFocus.current = node.contains(document.activeElement); };
  }, []);
  useLayoutEffect(() => {
    if (!heldFocus.current || panel) return;
    heldFocus.current = false;
    const { reference, origin } = opened.current ?? {};
    const segment = reference ? [...output.current?.querySelectorAll<HTMLElement>(".shelf-segment") ?? []].find((part) => part.dataset.reference === reference) : undefined;
    const target = segment?.querySelector("button") ?? (origin?.isConnected ? origin : output.current?.querySelector<HTMLElement>(".shelf-actions button")) ?? fallbackFocusRef?.current;
    target?.focus({ preventScroll: true });
  }, [panel, fallbackFocusRef]);
  const segmentKeys: (event: Parameters<NonNullable<ComponentProps<typeof Button>["onKeyDown"]>>[0], reference: CitationReference) => void = (event, reference) => {
    if (composing(event)) return event.continuePropagation();
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      event.preventDefault();
      const segments = [...output.current?.querySelectorAll<HTMLButtonElement>(".shelf-segment-button") ?? []];
      const step = (event.key === "ArrowRight" ? 1 : -1) * (getComputedStyle(event.currentTarget).direction === "rtl" ? -1 : 1);
      segments[Math.min(segments.length - 1, Math.max(0, segments.indexOf(event.currentTarget as HTMLButtonElement) + step))]?.focus();
    }
    // ↑ ↓ belong to the list, as in Search: go back there and move once, so the
    // shelf never holds focus for a key it does not use.
    else if ((event.key === "ArrowUp" || event.key === "ArrowDown") && !event.altKey && !event.metaKey && fallbackFocusRef?.current?.isConnected) {
      event.preventDefault();
      const search = fallbackFocusRef.current;
      search.focus({ preventScroll: true });
      search.dispatchEvent(new KeyboardEvent("keydown", { key: event.key, code: event.key, bubbles: true, cancelable: true }));
    }
    else if ((event.key === "Backspace" || event.key === "Delete") && isSelected(reference)) {
      event.preventDefault(); remove(reference);
    }
    // React Aria stops every key it is shown. Esc and the rest belong to the picker.
    else event.continuePropagation();
  };
  const keyText = (value: string) => keys ? value.split(/([^\p{L}\p{N}\s]+)/u).map((part, index) => index % 2 ? <span className="shelf-punctuation" key={index}>{part}</span> : part) : value;
  const segments = visible?.spans ?? (shown === undefined || shown === null && !previous ? references.map((_, item) => ({ item, text: `${item ? "; " : ""}${labels[item]}` })) : undefined);
  const manySegments = !single && !copied && segments?.map((span, index) => {
    const reference = span.item === null ? undefined : references[span.item];
    const contents = !keys && "runs" in span && span.runs ? renderRuns(span.runs) : keyText(span.text);
    if (!reference || !onChangeReference || copied) return <span key={index} className="shelf-shared">{contents}</span>;
    const name = labels[span.item!]!;
    return <ShelfSegment key={idKey(reference.id)} reference={idKey(reference.id)} referenceKey={reference.key} aria-label={citationDetailsText.edit(name)}
      aria-describedby={`${citationId}-${index}`} aria-keyshortcuts={isSelected(reference) ? "Backspace Delete" : undefined}
      aria-expanded={inspecting(reference)} aria-controls={inspecting(reference) ? inspectorId : undefined}
      onKeyDown={(event) => segmentKeys(event, reference)} onPress={(event) => edit(reference, event.target as HTMLElement)}
      remove={isSelected(reference) ? { label: removeLabel(reference), onPress: () => remove(reference) } : undefined}>
      <bdi id={`${citationId}-${index}`} dir="auto" className="preview-text">{contents}</bdi>
    </ShelfSegment>;
  });
  const singleSegment = single && onChangeReference && !copied ? <ShelfSegment reference={idKey(single.id)} referenceKey={single.key} aria-label={citationDetailsText.edit(labels[0]!)}
    aria-keyshortcuts={isSelected(single) ? "Backspace Delete" : undefined}
    aria-describedby={text !== undefined ? citationId : undefined}
    aria-expanded={inspecting(single)} aria-controls={inspecting(single) ? inspectorId : undefined}
    onKeyDown={(event) => segmentKeys(event, single)} onPress={(event) => edit(single, event.target as HTMLElement)}
    remove={isSelected(single) ? { label: removeLabel(single), onPress: () => remove(single) } : undefined}>
    <bdi id={text !== undefined ? citationId : undefined} dir="auto" className={text !== undefined ? "preview-text" : "shelf-reference-name"}>{text !== undefined ? keys ? keyText(text) : visible && visible.markup.length ? render(visible.markup) : text : labels[0]}</bdi>
  </ShelfSegment> : null;
  return <>
    <p className="visually-hidden" role="status" aria-label={pickerText.citationSelection} aria-live="polite" aria-atomic="true">
      {announce ? [pickerText.selectionCount(selected.size), ...partialWarnings].join(" ") : ""}
    </p>
    {panel && hasShelf && <CitationInspector key={panel.reference ?? panel.kind} ref={returnFocus} id={inspectorId} panel={panel} heading={labels[editedIndex] ?? pickerText.citationDetails}
      reference={panel.kind === "warnings" ? undefined : edited} index={editedIndex} selected={Boolean(edited && isSelected(edited))} previewReceipt={previewReceipt} output={`${style}:${keys}`} citation={visible}
      failure={shown === "failed" ? fullProblem : copyProblem ?? null} notes={{ entry: entryNotice, output: currentWarnings, closest: partialWarnings }} source={source} keys={keys}
      onChange={onChangeReference} onSubmit={onSubmit} onOpenSource={onOpenSource} onRemove={remove} onClose={close} />}
    {hasShelf ? <div className="shelf" data-folded={compact || undefined} data-copied={copied ? "" : undefined} ref={preserveFocus}>
      <section ref={(node) => { output.current = node; }} aria-label={shown !== undefined || copied ? pickerText.preview : pickerText.citationDetails}
        tabIndex={focusable && shown !== undefined && !copied && !singleSegment && !manySegments ? 0 : undefined}
        className="shelf-output" data-keys={keys || undefined} data-state={!copied && (shown === null || typeof shown === "string") ? shown ?? "pending" : undefined}>
        <span className="shelf-leading"><Beak /></span>
        <div className="shelf-citation" role={selected.size ? "group" : undefined} aria-label={selected.size ? pickerText.selectedReferences : undefined}>
          {previous && !retainedSegments ? <bdi className="preview-text shelf-retained" dir="auto">{keyText(previous.text)}</bdi> : shown !== "failed" && shown !== "stale" && singleSegment}
          {text !== undefined ? singleSegment ? null : manySegments ? <span className="shelf-fragments" dir={keys ? "ltr" : "auto"}>{manySegments}</span> : <bdi id={citationId} dir="auto" className="preview-text">
            {ready?.markup.length && !keys ? render(ready.markup) : keyText(text)}
          </bdi> : shown === "failed" ? <><span className="shelf-state-icon" data-state="failed" aria-hidden="true">!</span><p title={fullProblem}>{fullProblem}</p></> : shown === "stale" ? <><span className="shelf-state-icon" data-state="stale" aria-hidden="true">!</span><p role="status">{pickerText.shelfStale}</p></>
            : shown === undefined && !previous && manySegments ? <span className="shelf-fragments">{manySegments}</span> : shown === null && !previous ? <p role="status">{pickerText.previewPending}</p> : null}
          {copied && <Beak closing />}
        </div>
        <div className="shelf-actions">
          {copied && <span role="status" aria-label={pickerText.copied} className="shelf-copied"><Icon name="check" />{pickerText.copied}</span>}
          {!copied && references.length > 1 && <ToolbarMenu buttonRef={count} label={pickerText.selectedReferences} value={String(references.length)} placement="top end" selectedKey={panel?.reference ?? ""}
            button={{ className: "shelf-count", quiet: true, label: interfaceText("shelf-references", { count: references.length }), title: pickerText.allChosen(references.length),
              // Words say what the count is; a folded shelf keeps the number, as its warnings do.
              content: compact ? String(references.length) : interfaceText("shelf-references", { count: references.length }) }}
            filterLabel={pickerText.filterReferences} sections={[{ id: "references", label: pickerText.selectedReferences, items: references.map((reference, i) => ({ id: idKey(reference.id), label: labels[i]!,
              sample: [reference.locator && `${reference.locator.label} ${reference.locator.value}`, reference.partial && pickerText.closestMatch].filter(Boolean).join(" · ") || undefined, prose: true })) }]}
            onAction={(key) => { const reference = references.find((r) => idKey(r.id) === key); if (reference) open("reference", count.current!, reference); }} />}
          {statusLabel && !detailsOpen && <Button variant="quiet" className="shelf-warnings" aria-label={statusLabel} title={statusLabel}
            aria-expanded={panel?.kind === "warnings"} aria-controls={panel?.kind === "warnings" ? inspectorId : undefined}
            onPress={(event) => toggle("warnings", event.target as HTMLElement)}>{compact ? warnings.length > 0 ? `! ${warnings.length}` : "i" : <>
              {warnings.length > 0 && <span>{pickerText.warningCount(warnings.length)}</span>}
              {entryNotice && <span>{pickerText.entryOnlyShort}</span>}
            </>}</Button>}
          {!copied && !detailsOpen && (shown === "failed" || shown === "stale") && <Button variant="quiet" aria-expanded={false}
            onPress={(event) => open("failure", event.target as HTMLElement, references[0])}>{pickerText.details}</Button>}
        </div>
      </section>
    </div> : null}
  </>;
}
