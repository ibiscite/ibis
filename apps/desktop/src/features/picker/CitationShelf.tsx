import { useCallback, useLayoutEffect, useRef, useState, type RefObject } from "react";
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
import { beak } from "../../brand";
import "./CitationShelf.css";

/** The shelf shows at most this many chips; the rest fold into "+N"
 * (docs/design.md, "Shelf"). */
export const SHELF_CHIPS = 6;

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
export function CitationShelf({ announce, selected, highlightedReference, onChangeReference, removeLabel, onRemove, onRemoveLocator, style, keys, shown, problem, focusable, fallbackFocusRef }: {
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
  const warnings = [...(shown && typeof shown === "object" ? shown.warnings : []), ...partialWarnings];
  const moreLabel = (count: number) => records.slice(records.length - count).some((r) => r.partial)
    ? `${pickerText.moreChosen(count)} · ${pickerText.closestMatches}` : pickerText.moreChosen(count);
  // More than SHELF_CHIPS fold into "+N", which shows every chip on a few
  // scrolling lines, so each chosen reference can always be removed here.
  const [expanded, setExpanded] = useState(false);
  const [visibleChips, setVisibleChips] = useState(SHELF_CHIPS);
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
    const list = chips.current;
    if (!list) return;
    const fit = () => {
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
        if (!expanded && [...list.querySelectorAll(".chip")].slice(next).some((chip) => chip.contains(document.activeElement))) button?.focus();
      }
      setVisibleChips(next);
    };
    fit();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(fit);
    observer?.observe(list);
    for (const node of sizes.current?.children ?? []) observer?.observe(node);
    window.addEventListener("resize", fit);
    return () => { observer?.disconnect(); window.removeEventListener("resize", fit); };
  }, [selected, expanded, visibleChips]);
  if (expanded && selected.size === 0) setExpanded(false);
  const folded = Math.max(0, records.length - visibleChips);
  // Keep the first chips measurable even if none fit; hidden chips are neither
  // exposed nor focusable. Expanding still renders every record in source order.
  const shownChips = expanded ? records : records.slice(0, SHELF_CHIPS);
  return (
    <>
      <p className="visually-hidden" role="status" aria-label={pickerText.citationSelection} aria-live="polite" aria-atomic="true">
        {announce ? [pickerText.selectionCount(selected.size), ...partialWarnings].join(" ") : ""}
      </p>
      {(records.length > 0 || shown !== undefined || edited) && (
        <div className="shelf" ref={preserveFocus}>
          {records.length > 0 && (
            <ul ref={chips} className="shelf-chips" aria-label={pickerText.selectedReferences} data-expanded={(folded > 0 && expanded) || undefined}>
              <li ref={sizes} className="chip-more chip-fit-measure" aria-hidden="true" inert>
                {Array.from({ length: Math.min(records.length, SHELF_CHIPS + 1) }, (_, i) => (
                  <Button key={i} variant="quiet" size="small" excludeFromTabOrder>{moreLabel(records.length - i)}</Button>
                ))}
              </li>
              {shownChips.map((r, i) => (
                <li key={keysOf[i]} ref={preserveFocus} className="chip" title={r.key} data-partial={r.partial || undefined} hidden={!expanded && i >= visibleChips}
                  onFocusCapture={() => setEditing(keysOf[i]!)}>
                  {onChangeReference ? <Button variant="quiet" size="small" aria-label={citationDetailsText.edit(labels[i]!)}
                    aria-pressed={edited === r} onPress={() => { setEditing(keysOf[i]!); setEditRequest((request) => request + 1); }}><bdi className="chip-label">{labels[i]}</bdi></Button>
                    : <bdi className="chip-label">{labels[i]}</bdi>}
                  {r.partial && <span className="shelf-pill" title={pickerText.closestMatchWarning(r.title || r.key)}>
                    <span aria-hidden="true">{pickerText.closestMatch}</span>
                    <span className="visually-hidden">{pickerText.closestMatchWarning(r.title || r.key)}</span>
                  </span>}
                  {r.locator && onRemoveLocator && <LocatorToken text={`${r.locator.label} ${r.locator.value}`}
                    removeLabel={pickerText.removeLocator(r.locator.label, r.locator.value, r.key)} onRemove={() => onRemoveLocator(r)} />}
                  <Button variant="icon" size="small" className="chip-remove" onPress={() => onRemove(r)} aria-label={removeLabel(r)} />
                </li>
              ))}
              {folded > 0 && (
                <li ref={moreRef} className="chip-more">
                  <Button variant="quiet" size="small" aria-expanded={expanded} onPress={() => setExpanded(!expanded)}
                    title={!expanded ? closestMatchWarnings(records.slice(visibleChips)).join(" ") || undefined : undefined}
                    aria-label={expanded ? pickerText.fewerChosen : pickerText.allChosen(records.length)}>
                    {expanded ? pickerText.fewerChosenShort : moreLabel(folded)}
                  </Button>
                </li>
              )}
            </ul>
          )}
          {edited && onChangeReference && <CitationDetails key={idKey(edited.id)} pageRef={page} label={citationDetailsText.details(edited.title || edited.key)}
            page={edited.locator?.label === "page" ? edited.locator.value : ""} form={edited.form ?? "normal"}
            onPageChange={(value) => onChangeReference(edited, { locator: value.trim() ? { label: "page", value, input: "page-editor" } : null })}
            onFormChange={(form) => onChangeReference(edited, { form })} />}
          {shown !== undefined && (
            <section ref={preserveFocus} aria-label={pickerText.preview} className="shelf-output" data-keys={keys || undefined}
              data-state={shown === null || typeof shown === "string" ? (shown ?? "pending") : undefined}
              tabIndex={focusable && shown && typeof shown === "object" ? 0 : undefined}>
              {/* Static generated artwork, never user or library markup. */}
              <svg className="shelf-mark" viewBox={beak.viewBox} fill="currentColor" aria-hidden="true" focusable="false"
                preserveAspectRatio="xMaxYMid meet" dangerouslySetInnerHTML={{ __html: beak.body }} />
              {shown === null && <p role="status">{pickerText.previewPending}</p>}
              {shown === "failed" && <p role="alert">{problem ?? pickerText.previewError}</p>}
              {shown === "stale" && <p role="status">{pickerText.shelfStale}</p>}
              {shown && typeof shown === "object" && (
                <>
                  <bdi dir={keys ? "ltr" : "auto"} className="preview-text" title={shown.text}>
                    {shown.markup.length > 0 ? render(shown.markup) : shown.text}
                  </bdi>
                  {style && copiesEntry(style) && (
                    <span className="shelf-pill" title={pickerText.entryOnly(STYLE_NAMES[style])}>
                      <span aria-hidden="true">{pickerText.entryOnlyShort}</span>
                      <span className="visually-hidden">{pickerText.entryOnly(STYLE_NAMES[style])}</span>
                    </span>
                  )}
                  {warnings.length > 0 && (
                    <span className="shelf-pill" title={warnings.join("\n")}>
                      <span aria-hidden="true">{pickerText.warningCount(warnings.length)}</span>
                      <span className="visually-hidden">{warnings.join(" ")}</span>
                    </span>
                  )}
                </>
              )}
            </section>
          )}
        </div>
      )}
    </>
  );
}
