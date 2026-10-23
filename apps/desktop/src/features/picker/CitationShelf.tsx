import { useLayoutEffect, useRef, useState, type Ref } from "react";
import type { SearchResult } from "../../api";
import { provisional, type FormattedCitation } from "../../format/formatted";
import { render } from "../../format/markup";
import type { StyleId } from "../../format/styles";
import { pickerText } from "../../messages";
import { Button } from "../../ui/Button";
import { beak } from "../../brand";
import "./CitationShelf.css";

/** The shelf shows at most this many chips; the rest fold into "+N"
 * (docs/design/preview.md, "Chips before the count"). */
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

/** What Enter pastes, above the footer's Copy action (docs/design/preview.md):
 * a line of chips for the references chosen with Shift+Enter, then the exact
 * output on one line. Results above never move when a reference is added. */
export function CitationShelf({ announce, selected, removeLabel, onRemove, style, keys, shown, focusable, outputRef }: {
  /** False until the selection first changes, so opening the picker is silent. */
  announce: boolean;
  selected: Map<string, SearchResult>;
  removeLabel: (r: SearchResult) => string;
  onRemove: (r: SearchResult) => void;
  /** The citation style, when one is chosen. */
  style: StyleId | null;
  /** True for a citation-key syntax, which is shown in monospace. */
  keys: boolean;
  /** What Enter would copy; null while it is being made, undefined when there is nothing to show. */
  shown: FormattedCitation | "failed" | "stale" | null | undefined;
  /** A built citation can be focused, to read it whole. */
  focusable: boolean;
  outputRef?: Ref<HTMLElement>;
}) {
  const records = [...selected.values()];
  const keysOf = [...selected.keys()];
  const labels = chipLabels(records);
  // More than SHELF_CHIPS fold into "+N", which shows every chip on a few
  // scrolling lines, so each chosen reference can always be removed here.
  const [expanded, setExpanded] = useState(false);
  const [visibleChips, setVisibleChips] = useState(SHELF_CHIPS);
  const chips = useRef<HTMLUListElement>(null);
  const more = useRef<HTMLLIElement>(null);
  const sizes = useRef<HTMLLIElement>(null);
  useLayoutEffect(() => {
    const list = chips.current;
    if (!list) return;
    const fit = () => {
      const first = list.querySelector<HTMLElement>(".chip");
      if (!first || list.clientWidth <= 0) return;
      const style = getComputedStyle(list);
      const minimum = parseFloat(getComputedStyle(first).minWidth);
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
        {announce ? pickerText.selectionCount(selected.size) : ""}
      </p>
      {(records.length > 0 || shown !== undefined) && (
        <div className="shelf">
          {records.length > 0 && (
            <ul ref={chips} className="shelf-chips" aria-label={pickerText.selectedReferences} data-expanded={(folded > 0 && expanded) || undefined}>
              <li ref={sizes} className="chip-more chip-fit-measure" aria-hidden="true" inert>
                {Array.from({ length: Math.min(records.length, SHELF_CHIPS + 1) }, (_, i) => (
                  <Button key={i} variant="quiet" size="small" excludeFromTabOrder>{pickerText.moreChosen(records.length - i)}</Button>
                ))}
              </li>
              {shownChips.map((r, i) => (
                <li key={keysOf[i]} className="chip" title={r.key} hidden={!expanded && i >= visibleChips}>
                  <bdi className="chip-label">{labels[i]}</bdi>
                  <Button variant="icon" size="small" className="chip-remove" onPress={() => onRemove(r)} aria-label={removeLabel(r)} />
                </li>
              ))}
              {folded > 0 && (
                <li ref={more} className="chip-more">
                  <Button variant="quiet" size="small" aria-expanded={expanded} onPress={() => setExpanded(!expanded)}
                    aria-label={expanded ? pickerText.fewerChosen : pickerText.allChosen(records.length)}>
                    {expanded ? pickerText.fewerChosenShort : pickerText.moreChosen(folded)}
                  </Button>
                </li>
              )}
            </ul>
          )}
          {shown !== undefined && (
            <section ref={outputRef} aria-label={pickerText.preview} className="shelf-output" data-keys={keys || undefined}
              data-state={shown === null || typeof shown === "string" ? (shown ?? "pending") : undefined}
              tabIndex={focusable && shown && typeof shown === "object" ? 0 : undefined}>
              {/* Static generated artwork, never user or library markup. */}
              <svg className="shelf-mark" viewBox={beak.viewBox} fill="currentColor" aria-hidden="true" focusable="false"
                preserveAspectRatio="xMaxYMid meet" dangerouslySetInnerHTML={{ __html: beak.body }} />
              {shown === null && <p role="status">{pickerText.previewPending}</p>}
              {shown === "failed" && <p role="alert">{pickerText.previewError}</p>}
              {shown === "stale" && <p role="status">{pickerText.shelfStale}</p>}
              {shown && typeof shown === "object" && (
                <>
                  <bdi dir={keys ? "ltr" : "auto"} className="preview-text" title={shown.text}>
                    {shown.markup.length > 0 ? render(shown.markup) : shown.text}
                  </bdi>
                  {style && provisional(style) && (
                    <span className="shelf-pill" title={pickerText.provisional}>
                      <span aria-hidden="true">{pickerText.provisionalShort}</span>
                      <span className="visually-hidden">{pickerText.provisional}</span>
                    </span>
                  )}
                  {shown.warnings.length > 0 && (
                    <span className="shelf-pill" title={shown.warnings.join("\n")}>
                      <span aria-hidden="true">{pickerText.warningCount(shown.warnings.length)}</span>
                      <span className="visually-hidden">{shown.warnings.join(" ")}</span>
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
