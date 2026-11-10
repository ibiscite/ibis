import { Icon } from "../../icons";
import { useCallback, useEffect, useId, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, type ComponentProps, type Ref, type RefObject } from "react";
import type { SearchResult } from "../../api";
import type { FilePreview, Notice } from "../../bindings";
import { outputName, writesEntries, type FormattedCitation, type OutputId } from "../../format/outputs";
import { render, renderRuns } from "../../format/markup";
import { say } from "../../i18n";
import { choiceKey, idKey, type CitationDetailsChange, type CitationReference } from "./choice";
import { closestMatchWarnings } from "./rows";
import type { ShelfCopied } from "./session";
import { composing } from "../../composition";
import { Button } from "../../ui/Button";
import { CitationToken } from "../../ui/CitationToken";
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

/** What More and the keys do to the citation's tokens: the focused one, or the last. */
export type CitationLineActions = {
  /** Has tokens; `selected` when at least one is in the citation, so it can move or go. */
  state: () => { tokens: number; selected: number };
  focusLast: () => void; edit: () => void; move: (step: -1 | 1) => void; remove: () => void;
};

/** Whether a reference's words in the output name it, or it needs its handle beside them. */
function names(words: string, reference: SearchResult): boolean {
  const text = words.toLocaleLowerCase();
  return [reference.first_author, reference.key].some((name) => name && text.includes(name.toLocaleLowerCase()));
}

function Beak({ closing = false }: { closing?: boolean }) {
  return <svg className={`shelf-mark shelf-mark-${closing ? "close" : "open"}`} viewBox={beak.viewBox} fill="currentColor" aria-hidden="true" focusable="false"
    dangerouslySetInnerHTML={{ __html: beak.body }} />;
}

/** One line of exact engine output. All editing stays in the inspector above it;
 * a folded citation retains its full text and every reference's identity. */
export function CitationShelf({ actionsRef, onMoveReference, onKeepOrder, deliverable = true, keepOrder = false, inspector, onSubmit, copyProblem, announce, selected, highlightedReference, highlightedReferences, onChangeReference, onRemove, referenceLibrary, style, keys, shown, previewReceipt, outputWarnings, outputNotices = [], problem, focusable, fallbackFocusRef, presentation, present = false, copied: copiedReceipt }: {
  inspector: CitationInspectorState;
  actionsRef?: Ref<CitationLineActions | null>;
  /** Moves a reference a place along the line's order (`shown`), as the session does. */
  onMoveReference?: (reference: CitationReference, shown: readonly string[], step: -1 | 1) => void;
  onKeepOrder?: (on: boolean) => void;
  /** Enter in Page does what it does in Search. */
  onSubmit?: () => void;
  copyProblem?: string | null;
  /** Enter would deliver the shown citation now; until then nothing drags. */
  deliverable?: boolean;
  keepOrder?: boolean;
  presentation?: number;
  present?: boolean;
  announce: boolean;
  selected: Map<string, CitationReference>;
  highlightedReference?: CitationReference;
  highlightedReferences?: CitationReference[];
  previewReceipt?: FilePreview;
  onChangeReference?: (reference: CitationReference, change: CitationDetailsChange) => void;
  onRemove: (r: SearchResult) => void;
  referenceLibrary?: (r: SearchResult) => string | null;
  style: OutputId | null;
  keys: boolean;
  shown: FormattedCitation | "failed" | "stale" | null | undefined;
  outputWarnings: readonly string[];
  /** The engine's notices about the shown citation. */
  outputNotices?: readonly Notice[];
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
  const { panel, show, close } = inspector;
  const inspectorId = useId();
  const citationId = useId();
  const output = useRef<HTMLElement>(null);
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
  // What depends on the whole document: a note style's full first note, from the
  // engine's notice about the citation shown (an editor's output included), or
  // a numbered style's entry.
  const entryNotice = outputNotices.includes("full-first-note") ? { short: say("picker-first-note-short"), text: say("picker-first-note") }
    : style && writesEntries(style) ? { short: say("picker-entry-only-short"), text: say("picker-entry-only", { style: outputName(style) }) } : null;
  // The line holds only the citation: what the whole document decides is said in
  // the footer's words and the details, never as a label here.
  const statusLabel = [warnings.length > 0 && say("picker-warning-count", { count: warnings.length }),
    partialWarnings.length > 0 && say("picker-closest-matches")].filter(Boolean).join(" · ");
  const open = (kind: InspectorPanel["kind"], origin: HTMLElement, reference?: CitationReference) => show({ kind, origin, presentation, receipt: copied, reference: reference && idKey(reference.id) });
  const inspecting = (reference: CitationReference) => panel?.kind === "reference" && panel.reference === idKey(reference.id);
  // As with Mail's tokens, a segment selects what the details show; pressed again, it closes them.
  const edit = (reference: CitationReference, origin: HTMLElement) => { if (inspecting(reference)) close(); else open("reference", origin, reference); };
  const toggle = (kind: InspectorPanel["kind"], origin: HTMLElement, reference?: CitationReference) => { if (panel?.kind === kind) close(); else open(kind, origin, reference); };
  const remove = (reference: CitationReference) => { close(); onRemove(reference); };
  const isSelected = (reference: CitationReference) => records.some((r) => idKey(r.id) === idKey(reference.id));
  const fullProblem = problem ?? (style ? say("picker-preview-style-error", { style: outputName(style) }) : say("picker-preview-error"));
  const ready = shown && typeof shown === "object" ? shown : null;
  const currentIds = references.map((reference) => idKey(reference.id));
  const referenceIds = JSON.stringify(currentIds);
  const retained = useRef<{ presentation: typeof presentation; ids: readonly string[]; value: FormattedCitation } | null>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: referenceIds stands for currentIds.
  useLayoutEffect(() => { if (ready) retained.current = { presentation, ids: currentIds, value: ready }; }, [ready, presentation, referenceIds]);
  const meaning = choiceKey(references);
  const pendingOwner = useMemo(() => ({ meaning, keepOrder, style, keys, presentation }), [meaning, keepOrder, style, keys, presentation]);
  const [expired, setExpired] = useState<object | null>(null);
  // While the next preview is made, hold the last citation for 300 ms.
  const waiting = shown === null;
  useEffect(() => {
    if (!waiting) { setExpired(null); return; }
    const timer = window.setTimeout(() => setExpired(pendingOwner), 300);
    return () => window.clearTimeout(timer);
  }, [waiting, pendingOwner]);
  const previous = waiting && expired !== pendingOwner && retained.current?.presentation === presentation ? retained.current?.value ?? null : null;
  // The same references in another order keep their tokens while the next preview is
  // made, each found by identity, so a moved token keeps its place and its focus.
  const retainedIds = retained.current?.ids ?? [];
  const retainedSegments = Boolean(previous && retainedIds.length === currentIds.length && currentIds.every((id) => retainedIds.includes(id)));
  const visible = ready ?? (retainedSegments ? previous : null);
  const text = visible?.text;
  // Dragging delivers what Enter would: only the current prepared citation,
  // never one held while the next is made, and never a refused one (0028).
  const dragged = deliverable && !copied && ready && !ready.project_refusal ? ready : null;
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
    const segment = reference ? [...output.current?.querySelectorAll<HTMLElement>(".citation-token") ?? []].find((part) => part.dataset.reference === reference) : undefined;
    const target = segment?.querySelector("button") ?? (origin?.isConnected ? origin : output.current?.querySelector<HTMLElement>(".shelf-actions button")) ?? fallbackFocusRef?.current;
    target?.focus({ preventScroll: true });
  }, [panel, fallbackFocusRef]);
  // The details grow from their reference's token, or from the control that opened them.
  const anchor = useRef<Element | null>(null);
  useLayoutEffect(() => {
    anchor.current = (panel?.kind === "reference" && panel.reference ? tokenButton(panel.reference) : null) ?? (panel?.origin.isConnected ? panel.origin : output.current);
  });
  const fromPrevious = !ready && retainedSegments;
  const referenceAt = (item: number) => fromPrevious ? references.find((reference) => idKey(reference.id) === retainedIds[item]) : references[item];
  const keyText = (value: string) => keys ? value.split(/([^\p{L}\p{N}\s]+)/u).map((part, index) => index % 2 ? <span className="shelf-punctuation" key={index}>{part}</span> : part) : value;
  const segments = visible?.spans ?? (shown === undefined || shown === null && !previous ? references.map((_, item) => ({ item, text: `${item ? "; " : ""}${labels[item]}` })) : undefined);
  // The line's order, which a style may have sorted: moves follow what the person sees.
  const shownOrder = [...new Set(segments?.flatMap((span) => span.item === null ? [] : [referenceAt(span.item)]).filter((r) => r !== undefined).map((r) => idKey(r.id)))];
  // With the order kept (always, for keys) the line is the person's order, which
  // a move changes at once, so quick moves compose before the next preview shows.
  const lineOrder = keepOrder || keys || shownOrder.length !== currentIds.length ? currentIds : shownOrder;
  const tokenButton = (key: string) => output.current?.querySelector<HTMLButtonElement>(`.citation-token[data-reference="${CSS.escape(key)}"] > button`) ?? null;
  // A moved token is found again after the line reorders, until the next preview shows it.
  const refocus = useRef<{ key: string; ready: FormattedCitation | null } | null>(null);
  const [orderNote, setOrderNote] = useState("");
  const move = (reference: CitationReference, step: -1 | 1) => {
    const index = lineOrder.indexOf(idKey(reference.id));
    if (!onMoveReference || !isSelected(reference) || index + step < 0 || index + step >= lineOrder.length) return;
    refocus.current = document.activeElement === tokenButton(idKey(reference.id)) ? { key: idKey(reference.id), ready } : null;
    if (!keepOrder && style) setOrderNote(say("picker-order-kept"));
    onMoveReference(reference, lineOrder, step);
  };
  // The token last focused, for More's commands once focus has moved into the menu;
  // focus going anywhere else forgets it, and cancels a pending refocus.
  const remembered = useRef<string | null>(null);
  useEffect(() => {
    const focused = (event: FocusEvent) => {
      const node = event.target instanceof HTMLElement ? event.target : null;
      const token = node?.closest<HTMLElement>(".citation-token")?.dataset.reference ?? null;
      if (token) remembered.current = token;
      else if (!node?.closest('[role="menu"], [role="dialog"]')) remembered.current = null;
      if (refocus.current && token !== refocus.current.key && node !== document.body) refocus.current = null;
    };
    document.addEventListener("focusin", focused);
    return () => document.removeEventListener("focusin", focused);
  }, []);
  useLayoutEffect(() => {
    const moved = refocus.current;
    if (!moved) return;
    const button = tokenButton(moved.key);
    if (button && document.activeElement !== button && (document.activeElement === document.body || output.current?.contains(document.activeElement))) button.focus({ preventScroll: true });
    if (ready && ready !== moved.ready) refocus.current = null;
  });
  // A changed order slides each token from where it was, in the line's time.
  const placed = useRef<{ order: string; at: Map<string, number> }>({ order: "", at: new Map() });
  useLayoutEffect(() => {
    const nodes = [...output.current?.querySelectorAll<HTMLElement>(".citation-token") ?? []];
    const order = nodes.map((node) => node.dataset.reference).join("\u0001");
    const at = new Map(nodes.map((node) => [node.dataset.reference!, node.getBoundingClientRect().left]));
    const { order: before, at: was } = placed.current;
    placed.current = { order, at };
    // The motion tokens answer Reduce Motion: there the slide takes no time.
    const motion = getComputedStyle(document.documentElement), duration = parseFloat(motion.getPropertyValue("--dur-2"));
    if (order === before || !duration) return;
    for (const node of nodes) {
      const from = was.get(node.dataset.reference!), to = at.get(node.dataset.reference!)!;
      if (from !== undefined && Math.abs(from - to) > 1 && typeof node.animate === "function") {
        node.animate([{ translate: `${from - to}px 0` }, { translate: "0 0" }], { duration, easing: motion.getPropertyValue("--ease-spring").trim() || "ease-out" });
      }
    }
  });
  const segmentKeys: (event: Parameters<NonNullable<ComponentProps<typeof Button>["onKeyDown"]>>[0], reference: CitationReference) => void = (event, reference) => {
    if (composing(event)) return event.continuePropagation();
    const plain = !event.altKey && !event.metaKey && !event.ctrlKey;
    if ((event.key === "ArrowLeft" || event.key === "ArrowRight") && plain) {
      event.preventDefault();
      const step = (event.key === "ArrowRight" ? 1 : -1) * (getComputedStyle(event.currentTarget).direction === "rtl" ? -1 : 1) as -1 | 1;
      if (event.shiftKey) move(reference, step);
      else {
        const tokens = [...output.current?.querySelectorAll<HTMLButtonElement>(".citation-token-button") ?? []];
        tokens[Math.min(tokens.length - 1, Math.max(0, tokens.indexOf(event.currentTarget as HTMLButtonElement) + step))]?.focus();
      }
    }
    // ↓ opens the details, as a pop-up opens its menu; ↑ goes back up to Search.
    else if (event.key === "ArrowDown" && plain && !event.shiftKey) {
      event.preventDefault();
      if (!inspecting(reference)) open("reference", event.currentTarget as HTMLElement, reference);
    }
    else if (event.key === "ArrowUp" && plain && !event.shiftKey && fallbackFocusRef?.current?.isConnected) {
      event.preventDefault();
      fallbackFocusRef.current.focus({ preventScroll: true });
    }
    else if ((event.key === "Backspace" || event.key === "Delete") && isSelected(reference)) {
      event.preventDefault(); remove(reference);
    }
    // React Aria stops every key it is shown. Esc and the rest belong to the picker.
    else event.continuePropagation();
  };
  const target = () => {
    const focused = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>(".citation-token")?.dataset.reference;
    const key = [focused, remembered.current, panel?.kind === "reference" ? panel.reference : undefined, lineOrder.at(-1)]
      .find((id) => id && currentIds.includes(id));
    return references.find((reference) => idKey(reference.id) === key);
  };
  useImperativeHandle(actionsRef, () => ({
    state: () => ({ tokens: copied ? 0 : references.length, selected: copied ? 0 : records.length }),
    focusLast: () => { const last = lineOrder.at(-1); if (last) tokenButton(last)?.focus(); },
    edit: () => { const reference = target(); if (reference && !inspecting(reference)) open("reference", tokenButton(idKey(reference.id)) ?? output.current!, reference); },
    move: (step) => { const reference = target(); if (reference) move(reference, step); },
    remove: () => { const reference = target(); if (reference && isSelected(reference)) remove(reference); },
  }));
  // A token is the reference's own words; the brackets and separators the output
  // adds are shared and quieter, never part of a token, for one reference too.
  const spanned = Boolean(visible?.spans?.some((span) => span.item === null));
  const token = (reference: CitationReference, name: string, words: string, contents: React.ReactNode, describedBy: string | undefined, index?: number) =>
    <CitationToken key={idKey(reference.id)} reference={idKey(reference.id)} referenceKey={reference.key} aria-label={say("citation-details-edit", { reference: name })}
      handle={names(words, reference) ? undefined : referenceLabel(reference)} title={words}
      remove={isSelected(reference) ? { label: say("citation-token-remove", { reference: name }), onPress: () => remove(reference) } : undefined}
      aria-describedby={describedBy} aria-keyshortcuts={isSelected(reference) ? "Backspace Delete Shift+ArrowLeft Shift+ArrowRight ArrowDown" : "ArrowDown"}
      aria-expanded={inspecting(reference)} aria-controls={inspecting(reference) ? inspectorId : undefined}
      onFocus={(event) => (event.target as HTMLElement).closest(".citation-token")?.scrollIntoView?.({ block: "nearest", inline: "nearest" })}
      // Enter pastes from a token as it does everywhere; Space or a click opens its details.
      onKeyDown={(event) => segmentKeys(event, reference)} onPress={(event) => { if (event.key === "Enter") onSubmit?.(); else edit(reference, event.target as HTMLElement); }}>
      {index === undefined ? contents : <bdi id={`${citationId}-${index}`} dir="auto" className="preview-text">{contents}</bdi>}
    </CitationToken>;
  const manySegments = (!single || spanned) && !copied && segments?.map((span, index) => {
    const reference = span.item === null ? undefined : referenceAt(span.item);
    const contents = !keys && "runs" in span && span.runs ? renderRuns(span.runs) : keyText(span.text);
    if (!reference || !onChangeReference || copied) return <span key={index} className="shelf-shared">{contents}</span>;
    return token(reference, labels[references.indexOf(reference)]!, span.text, contents, `${citationId}-${index}`, index);
  });
  const singleWords = text ?? labels[0] ?? "";
  const singleSegment = single && !spanned && onChangeReference && !copied
    ? token(single, labels[0]!, singleWords, <bdi id={text !== undefined ? citationId : undefined} dir="auto" className={text !== undefined ? "preview-text" : "shelf-reference-name"}>{text !== undefined ? keys ? keyText(text) : visible && visible.markup.length ? render(visible.markup) : text : labels[0]}</bdi>, text !== undefined ? citationId : undefined)
    : null;
  return <>
    <p className="visually-hidden" role="status" aria-label={say("picker-citation-selection")} aria-live="polite" aria-atomic="true">
      {announce ? [say("picker-selection-count", { count: selected.size }), ...partialWarnings].join(" ") : ""}
    </p>
    <p className="visually-hidden" role="status" aria-live="polite">{keepOrder ? orderNote : ""}</p>
    {panel && hasShelf && <CitationInspector key={panel.reference ?? panel.kind} ref={returnFocus} id={inspectorId} panel={panel} anchor={anchor} heading={labels[editedIndex] ?? say("picker-citation-details")}
      keepOrder={style && onKeepOrder && references.length > 1 && edited && isSelected(edited) ? { checked: keepOrder, onChange: onKeepOrder } : null}
      reference={panel.kind === "warnings" ? undefined : edited} index={editedIndex} selected={Boolean(edited && isSelected(edited))} previewReceipt={previewReceipt} output={`${style}:${keys}`}
      failure={shown === "failed" ? fullProblem : copyProblem ?? null} notes={{ entry: entryNotice, output: currentWarnings, closest: partialWarnings }}
      onChange={onChangeReference} onSubmit={onSubmit} onRemove={remove} onClose={close} />}
    {hasShelf ? <div className="shelf" data-folded={compact || undefined} data-copied={copied ? "" : undefined} ref={preserveFocus}>
      <section ref={(node) => { output.current = node; }} aria-label={shown !== undefined || copied ? say("picker-preview") : say("picker-citation-details")}
        tabIndex={focusable && shown !== undefined && !copied && !singleSegment && !manySegments ? 0 : undefined}
        className="shelf-output" data-keys={keys || undefined} data-state={!copied && (shown === null || typeof shown === "string") ? shown ?? "pending" : undefined}>
        {/* The beak is the citation's proxy, as a Mac window's title icon is its
            document's: drag it into any app to drop exactly what Enter pastes. */}
        <span className="shelf-leading" draggable={Boolean(dragged?.text) || undefined} title={dragged?.text ? say("picker-drag-citation") : undefined}
          onDragStart={(event) => {
            if (!dragged?.text) return event.preventDefault();
            event.dataTransfer.effectAllowed = "copy";
            event.dataTransfer.setData("text/plain", dragged.text);
            if (!keys && dragged.html) event.dataTransfer.setData("text/html", dragged.html);
          }}><Beak /></span>
        <div className="shelf-citation" role={selected.size ? "group" : undefined} aria-label={selected.size ? say("picker-selected-references") : undefined}>
          {previous && !retainedSegments ? <bdi className="preview-text shelf-retained" dir="auto">{keyText(previous.text)}</bdi> : shown !== "failed" && shown !== "stale" && singleSegment}
          {text !== undefined ? singleSegment ? null : manySegments ? <span className="shelf-fragments" dir={keys ? "ltr" : "auto"}>{manySegments}</span> : <bdi id={citationId} dir="auto" className="preview-text">
            {ready?.markup.length && !keys ? render(ready.markup) : keyText(text)}
          </bdi> : shown === "failed" ? <><span className="shelf-state-icon" data-state="failed" aria-hidden="true">!</span><p title={fullProblem}>{fullProblem}</p></> : shown === "stale" ? <><span className="shelf-state-icon" data-state="stale" aria-hidden="true">!</span><p role="status">{say("picker-shelf-stale")}</p></>
            : shown === undefined && !previous && manySegments ? <span className="shelf-fragments">{manySegments}</span> : shown === null && !previous ? <p role="status">{say("picker-preview-pending")}</p> : null}
          {copied && <Beak closing />}
        </div>
        <div className="shelf-actions">
          {copied && <span role="status" aria-label={say("picker-copied")} className="shelf-copied"><Icon name="check" />{say("picker-copied")}</span>}
          {statusLabel && !detailsOpen && <Button variant="quiet" className="shelf-warnings" aria-label={statusLabel} title={statusLabel}
            aria-expanded={panel?.kind === "warnings"} aria-controls={panel?.kind === "warnings" ? inspectorId : undefined}
            onPress={(event) => toggle("warnings", event.target as HTMLElement)}>{compact ? warnings.length > 0 ? `! ${warnings.length}` : "i" : <>
              {warnings.length > 0 && <span>{say("picker-warning-count", { count: warnings.length })}</span>}
            </>}</Button>}
          {!copied && !detailsOpen && (shown === "failed" || shown === "stale") && <Button variant="quiet" aria-expanded={false}
            onPress={(event) => open("failure", event.target as HTMLElement, references[0])}>{say("picker-details")}</Button>}
        </div>
      </section>
    </div> : null}
  </>;
}
