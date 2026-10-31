import { useCallback, useId, useLayoutEffect, useRef, useState, type RefObject } from "react";
import type { SearchResult } from "../../api";
import type { FormattedCitation } from "../../format/styles";
import { copiesEntry } from "../../format/styles";
import { STYLE_NAMES } from "../../format/styles";
import { render } from "../../format/markup";
import type { StyleId } from "../../format/styles";
import { citationDetailsText, pickerText } from "../../messages";
import { idKey, type CitationDetailsChange, type CitationReference } from "./choice";
import { closestMatchWarnings } from "./rows";
import { LocatorToken } from "../../ui/LocatorToken";
import { Button } from "../../ui/Button";
import { CitationDetails } from "../../ui/CitationDetails";
import { DetailsMenu } from "../../ui/DetailsMenu";
import { beak } from "../../brand";
import "./CitationShelf.css";

/** The shelf shows at most this many chips; the rest fold into "+N"
 * (docs/design.md, "Shelf"). */
export const SHELF_CHIPS = 6;
type Panel = { kind: "references" | "reference" | "failure" | "warnings"; anchor: HTMLElement; presentation: number | undefined };

/** "García 2021": the first author's family name and the year, or the key
 * when the record names neither. */
export function chipLabel(r: SearchResult): string {
  const label = [r.first_author, r.year ?? ""].join(" ").trim();
  return label || r.key;
}

/** Chip labels, with the key added to any label that two chips share. */
export function chipLabels(records: SearchResult[]): string[] {
  const labels = records.map(chipLabel);
  return labels.map((label, i) =>
    labels.indexOf(label) !== labels.lastIndexOf(label) && label !== records[i]!.key ? `${label} · ${records[i]!.key}` : label);
}

/** What Enter pastes, above the footer's Copy action (docs/design.md, "Shelf"):
 * a line of chips for the references chosen with Shift+Enter, then the exact
 * output on one line. Results above never move when a reference is added. */
export function CitationShelf({ announce, selected, highlightedReference, onChangeReference, removeLabel, onRemove, onRemoveLocator, style, keys, shown, outputWarnings, problem, focusable, fallbackFocusRef, presentation }: {
  presentation?: number;
  /** False until the selection first changes, so opening the picker is silent. */
  announce: boolean;
  selected: Map<string, CitationReference>;
  highlightedReference?: CitationReference;
  onChangeReference?: (reference: CitationReference, change: CitationDetailsChange) => void;
  removeLabel: (r: SearchResult) => string;
  onRemove: (r: SearchResult) => void;
  onRemoveLocator?: (r: CitationReference) => void;
  /** The citation style, when one is chosen. */
  style: StyleId | null;
  /** True for a citation-key syntax, which is shown in monospace. */
  keys: boolean;
  /** What Enter would copy; null while it is being made, undefined when there is nothing to show. */
  shown: FormattedCitation | "failed" | "stale" | null | undefined;
  /** Current engine warnings remain available when citation text is hidden. */
  outputWarnings: readonly string[];
  /** Why the citation failed, when there is something the user can fix. */
  problem?: string;
  /** A built citation can be focused, to read it whole. */
  focusable: boolean;
  /** Where focus goes before a focused chip, overflow control or preview disappears. */
  fallbackFocusRef?: RefObject<HTMLElement | null>;
}) {
  const preserveFocus = useCallback((node: HTMLElement | null) => {
    if (!node) return;
    // Ref cleanup runs before removal, while focus still identifies its owner.
    return () => {
      if (node.contains(document.activeElement) && fallbackFocusRef?.current?.isConnected) {
        fallbackFocusRef.current.focus({ preventScroll: true });
      }
    };
  }, [fallbackFocusRef]);
  const records = [...selected.values()];
  const keysOf = [...selected.keys()];
  const [editing, setEditing] = useState<string | null>(null);
  const edited = (editing ? selected.get(editing) : undefined) ?? records[0] ?? highlightedReference;
  const page = useRef<HTMLInputElement>(null);
  const [editRequest, setEditRequest] = useState(0);
  useLayoutEffect(() => {
    // After the chosen editor mounts, bypass intervening reference controls.
    if (editRequest > 0) page.current?.focus({ preventScroll: true });
  }, [editRequest]);
  const labels = chipLabels(records);
  const partialWarnings = closestMatchWarnings(records);
  const warnings = [...outputWarnings, ...partialWarnings];
  const entryNotice = style && copiesEntry(style) ? pickerText.entryOnly(STYLE_NAMES[style]) : null;
  const statusLabel = [warnings.length > 0 && pickerText.warningCount(warnings.length), entryNotice && pickerText.entryOnlyShort,
    partialWarnings.length > 0 && pickerText.closestMatches].filter(Boolean).join(" · ");
  const moreLabel = (count: number) => records.slice(records.length - count).some((r) => r.partial)
    ? `${pickerText.moreChosen(count)} · ${pickerText.closestMatches}` : pickerText.moreChosen(count);
  const [panel, setPanel] = useState<Panel | null>(null);
  const menuId = useId();
  const [compact, setCompact] = useState(false);
  const band = useRef<HTMLDivElement>(null);
  const bandRef = useCallback((node: HTMLDivElement | null) => { band.current = node; return preserveFocus(node); }, [preserveFocus]);
  const output = useRef<HTMLElement>(null);
  const outputRef = useCallback((node: HTMLElement | null) => { output.current = node; return preserveFocus(node); }, [preserveFocus]);
  const summary = useRef<HTMLButtonElement>(null);
  const summaryRef = useCallback((node: HTMLButtonElement | null) => { summary.current = node; return preserveFocus(node); }, [preserveFocus]);
  const foldedFocus = useRef(false);
  const [visibleChips, setVisibleChips] = useState(SHELF_CHIPS);
  useLayoutEffect(() => {
    if (!panel) return;
    if (panel.presentation !== presentation || panel.kind === "references" && selected.size === 0) { setPanel(null); return; }
    if (panel.anchor.isConnected && !panel.anchor.closest("[hidden]")) return;
    // Folding or removing the originating segment keeps its open menu attached
    // to the visible citation/count, without remounting the focused editor.
    const anchor = summary.current ?? output.current ?? band.current;
    if (anchor && anchor !== panel.anchor) setPanel({ ...panel, anchor });
  });
  const chips = useRef<HTMLUListElement>(null);
  const more = useRef<HTMLLIElement>(null);
  const sizes = useRef<HTMLLIElement>(null);
  const moreRef = useCallback((node: HTMLLIElement | null) => {
    more.current = node;
    const cleanup = preserveFocus(node);
    return () => { cleanup?.(); more.current = null; };
  }, [preserveFocus]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: each of these changes which chips render, so the fit is measured again.
  useLayoutEffect(() => {
    const fit = () => {
      const textSize = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
      const nextCompact = window.innerHeight <= 24 * textSize || window.innerWidth <= 30 * textSize;
      if (nextCompact && !compact && chips.current?.contains(document.activeElement)) foldedFocus.current = true;
      setCompact(nextCompact);
      const list = chips.current;
      if (!list) return;
      const first = list.querySelector<HTMLElement>(".chip");
      if (!first || list.clientWidth <= 0) return;
      const style = getComputedStyle(list);
      const minimum = Math.max(parseFloat(getComputedStyle(first).minWidth),
        ...[...list.querySelectorAll<HTMLElement>(".chip[data-partial]")].map((chip) =>
          parseFloat(getComputedStyle(chip).maxWidth) || chip.getBoundingClientRect().width));
      if (!(minimum > 0)) return;
      const gap = parseFloat(style.columnGap) || 0;
      const available = list.clientWidth - (parseFloat(style.paddingInlineStart) || 0) - (parseFloat(style.paddingInlineEnd) || 0);
      const capacity = Math.min(selected.size, SHELF_CHIPS);
      let next = SHELF_CHIPS;
      if (selected.size > SHELF_CHIPS || capacity * minimum + (capacity - 1) * gap > available) {
        const button = more.current?.querySelector("button");
        // Reserve the widest possible label, independently of the preceding fit.
        // At most seven labels cover zero through six visible chips.
        const overflow = Math.max(0, ...[...(sizes.current?.children ?? [])].map((node) => node.getBoundingClientRect().width));
        next = Math.max(0, Math.min(capacity, Math.floor((available - overflow) / (minimum + gap))));
        if ([...list.querySelectorAll(".chip")].slice(next).some((chip) => chip.contains(document.activeElement))) button?.focus();
      }
      setVisibleChips(next);
    };
    fit();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(fit);
    if (band.current) observer?.observe(band.current);
    if (chips.current) observer?.observe(chips.current);
    for (const node of sizes.current?.children ?? []) observer?.observe(node);
    window.addEventListener("resize", fit);
    return () => { observer?.disconnect(); window.removeEventListener("resize", fit); };
  }, [selected, compact, visibleChips]);
  useLayoutEffect(() => {
    if (compact && foldedFocus.current) { foldedFocus.current = false; summary.current?.focus({ preventScroll: true }); }
  }, [compact]);
  const folded = Math.max(0, records.length - visibleChips);
  const open = (kind: Panel["kind"], anchor: HTMLElement) => setPanel({ kind, anchor, presentation });
  const edit = (key: string | null, trigger: HTMLElement, fromMenu = false) => {
    setEditing(key);
    setPanel((previous) => ({ kind: "reference", anchor: fromMenu && previous ? previous.anchor : trigger.closest<HTMLElement>(".chip") ?? output.current ?? trigger, presentation }));
    setEditRequest((request) => request + 1);
  };
  const chip = (r: CitationReference, i: number, overlay = false) => (
    <li key={keysOf[i]} ref={overlay ? undefined : preserveFocus} className="chip" title={r.key} data-partial={r.partial || undefined}
      hidden={!overlay && i >= visibleChips} onFocusCapture={() => setEditing(keysOf[i]!)}>
      {onChangeReference ? <Button variant="quiet" size="small" aria-label={citationDetailsText.edit(labels[i]!)}
        aria-haspopup="dialog" aria-expanded={panel?.kind === "reference" && editing === keysOf[i]}
        aria-controls={panel?.kind === "reference" && editing === keysOf[i] ? menuId : undefined}
        onPress={(event) => edit(keysOf[i]!, event.target as HTMLElement, overlay)}><bdi className="chip-label">{labels[i]}</bdi></Button>
        : <bdi className="chip-label">{labels[i]}</bdi>}
      {r.partial && <span className="shelf-pill" title={pickerText.closestMatchWarning(r.title || r.key)}>
        <span aria-hidden="true">{pickerText.closestMatch}</span>
        <span className="visually-hidden">{pickerText.closestMatchWarning(r.title || r.key)}</span>
      </span>}
      {r.locator && onRemoveLocator && <LocatorToken text={`${r.locator.label} ${r.locator.value}`}
        removeLabel={pickerText.removeLocator(r.locator.label, r.locator.value, r.key)} onRemove={() => onRemoveLocator(r)} />}
      <Button variant="icon" size="small" className="chip-remove" onPress={() => onRemove(r)} aria-label={removeLabel(r)} />
    </li>
  );
  const detailsTitle = panel?.kind === "warnings" ? pickerText.citationWarnings : panel?.kind === "references" ? pickerText.selectedReferences : edited
    ? citationDetailsText.details(edited.title || edited.key) : pickerText.citationDetails;
  const fullProblem = problem ?? pickerText.previewError;
  const showOutput = shown !== undefined || statusLabel || compact && records.length > 0 || records.length === 0 && edited && onChangeReference;
  return (
    <>
      <p className="visually-hidden" role="status" aria-label={pickerText.citationSelection} aria-live="polite" aria-atomic="true">
        {announce ? [pickerText.selectionCount(selected.size), ...partialWarnings].join(" ") : ""}
      </p>
      {(records.length > 0 || shown !== undefined || edited) && (
        <div className="shelf" data-folded={compact || undefined} ref={bandRef}>
          {records.length > 0 && (
            <ul ref={chips} className="shelf-chips" hidden={compact} aria-label={pickerText.selectedReferences}>
              <li ref={sizes} className="chip-more chip-fit-measure" aria-hidden="true" inert>
                {Array.from({ length: Math.min(records.length, SHELF_CHIPS + 1) }, (_, i) => (
                  <Button key={i} variant="quiet" size="small" excludeFromTabOrder>{moreLabel(records.length - i)}</Button>
                ))}
              </li>
              {records.slice(0, SHELF_CHIPS).map((r, i) => chip(r, i))}
              {folded > 0 && (
                <li ref={moreRef} className="chip-more">
                  <Button variant="quiet" size="small" aria-haspopup="dialog" aria-expanded={panel?.kind === "references"}
                    aria-controls={panel?.kind === "references" ? menuId : undefined} onPress={(event) => open("references", event.target as HTMLElement)}
                    title={closestMatchWarnings(records.slice(visibleChips)).join(" ") || undefined}
                    aria-label={pickerText.allChosen(records.length)}>{moreLabel(folded)}</Button>
                </li>
              )}
            </ul>
          )}
          {showOutput && (
            <section ref={outputRef} aria-label={shown !== undefined ? pickerText.preview : pickerText.citationDetails} className="shelf-output" data-keys={keys || undefined}
              data-state={shown === null || typeof shown === "string" ? (shown ?? "pending") : undefined}
              tabIndex={focusable && shown && typeof shown === "object" ? 0 : undefined}>
              {compact && records.length > 0 && <Button ref={summaryRef} variant="quiet" size="small" aria-haspopup="dialog" aria-expanded={panel?.kind === "references" || panel?.anchor === summary.current}
                aria-controls={panel?.kind === "references" || panel?.anchor === summary.current ? menuId : undefined}
                aria-label={[pickerText.allChosen(records.length), partialWarnings.length > 0 && pickerText.closestMatches].filter(Boolean).join(" · ")}
                onPress={(event) => open("references", event.target as HTMLElement)}>{pickerText.moreChosen(records.length)}</Button>}
              {shown !== undefined && <svg className="shelf-mark" viewBox={beak.viewBox} fill="currentColor" aria-hidden="true" focusable="false"
                preserveAspectRatio="xMaxYMid meet" dangerouslySetInnerHTML={{ __html: beak.body }} />}
              {shown === null && <p role="status">{pickerText.previewPending}</p>}
              {/* Notices announces and expands the same refusal; its reason also
                  stays beside the failed citation, including with text hidden. */}
              {shown === "failed" && <p title={fullProblem}>{fullProblem}</p>}
              {shown === "stale" && <p role="status">{pickerText.shelfStale}</p>}
              {shown && typeof shown === "object" && <>
                <bdi dir={keys ? "ltr" : "auto"} className="preview-text" title={shown.text}>
                  {shown.markup.length > 0 ? render(shown.markup) : shown.text}
                </bdi>
              </>}
              {statusLabel && <Button variant="quiet" size="small" className="shelf-warnings" aria-label={statusLabel} title={statusLabel}
                aria-haspopup="dialog" aria-expanded={panel?.kind === "warnings"} aria-controls={panel?.kind === "warnings" ? menuId : undefined}
                onPress={(event) => open("warnings", event.target as HTMLElement)}>
                {compact ? <span aria-hidden="true">{warnings.length > 0 ? `! ${warnings.length}` : "i"}</span> : <>
                  {entryNotice && <span className="shelf-pill" title={entryNotice}><span aria-hidden="true">{pickerText.entryOnlyShort}</span><span className="visually-hidden">{entryNotice}</span></span>}
                  {warnings.length > 0 && <span className="shelf-pill" title={warnings.join("\n")}><span aria-hidden="true">{pickerText.warningCount(warnings.length)}</span><span className="visually-hidden">{warnings.join(" ")}</span></span>}
                </>}
              </Button>}
              {shown === "failed" ? <Button variant="quiet" size="small" aria-haspopup="dialog" aria-expanded={panel?.kind === "failure"}
                aria-controls={panel?.kind === "failure" ? menuId : undefined} onPress={(event) => open("failure", output.current ?? event.target as HTMLElement)}>{pickerText.details}</Button>
                : edited && onChangeReference && <Button variant="quiet" size="small" aria-haspopup="dialog" aria-label={pickerText.editCitation}
                  aria-expanded={panel?.kind === "reference"} aria-controls={panel?.kind === "reference" ? menuId : undefined}
                  onPress={(event) => edit(keysOf[0] ?? null, event.target as HTMLElement)}>{pickerText.edit}</Button>}
            </section>
          )}
        </div>
      )}
      {panel && <DetailsMenu id={menuId} anchor={panel.anchor} returnFocusRef={fallbackFocusRef} title={detailsTitle}
        heading={panel.kind !== "references" && panel.kind !== "warnings" && edited ? edited.title || edited.key : detailsTitle} onClose={() => setPanel(null)}>
        {panel.kind !== "references" && panel.kind !== "warnings" && edited && <p className="shelf-detail-title">{edited.title || edited.key}</p>}
        {panel.kind === "references" && <ul className="shelf-details-list" aria-label={pickerText.selectedReferences}>{records.map((r, i) => chip(r, i, true))}</ul>}
        {shown === "failed" && <p role="alert" className="shelf-problem">{fullProblem}</p>}
        {panel.kind !== "warnings" && edited && onChangeReference && <CitationDetails key={idKey(edited.id)} pageRef={page} autoFocus={panel.kind !== "references"} label={citationDetailsText.details(edited.title || edited.key)}
          page={edited.locator?.label === "page" ? edited.locator.value : ""} form={edited.form ?? "normal"}
          onPageChange={(value) => onChangeReference(edited, { locator: value.trim() ? { label: "page", value, input: "page-editor" } : null })}
          onFormChange={(form) => onChangeReference(edited, { form })} />}
        {panel.kind === "warnings" && entryNotice && <><h3>{pickerText.entryOnlyShort}</h3><p>{entryNotice}</p></>}
        {panel.kind === "warnings" && outputWarnings.map((warning, index) => <p key={`output-${index}:${warning}`}>{warning}</p>)}
        {panel.kind === "warnings" && partialWarnings.length > 0 && <h3>{pickerText.closestMatches}</h3>}
        {(panel.kind === "warnings" ? partialWarnings : warnings).map((warning, index) => <p key={`warning-${index}:${warning}`}>{warning}</p>)}
      </DetailsMenu>}
    </>
  );
}
