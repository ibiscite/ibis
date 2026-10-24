import { useLayoutEffect, useRef, type Ref, type RefObject } from "react";
import type { SearchResult } from "../../api";
import { provisional, type FormattedCitation } from "../../format/formatted";
import { plainText, render, type Markup } from "../../format/markup";
import type { StyleId } from "../../format/styles";
import { pickerText } from "../../messages";
import { ChipGroup, type ChipItem } from "../../ui/ChipGroup";
import { beak } from "../../brand";
import "./CitationShelf.css";

/** A short spoken reference label, with a key fallback. */
export function chipLabel(r: SearchResult): string {
  return [r.first_author, r.year ?? ""].join(" ").trim() || r.key;
}
export function chipLabels(records: SearchResult[]): string[] {
  const labels = records.map(chipLabel);
  return labels.map((label, i) => labels.indexOf(label) !== labels.lastIndexOf(label) && label !== records[i]!.key ? `${label} · ${records[i]!.key}` : label);
}

type Piece = { content: Markup[]; before: Markup[]; after: Markup[] };
/** Only complete, byte-equal ownership may replace the canonical aggregate. */
function projection(shown: FormattedCitation | string | null | undefined, owners: readonly string[] | undefined, selected: Map<string, SearchResult>) {
  if (!shown || typeof shown !== "object" || !shown.segments || !owners || owners.length !== selected.size
    || plainText(shown.segments.flatMap((part) => part.markup)) !== shown.text) return null;
  const pieces = new Map<string, Piece>();
  let leading: Markup[] = [], last: Piece | undefined;
  for (const part of shown.segments) {
    if (part.item === null) (last ? last.after : leading).push(...part.markup);
    else {
      const id = owners[part.item];
      if (!id || !selected.has(id) || pieces.has(id)) return null;
      last = { content: part.markup, before: leading, after: [] };
      leading = []; pieces.set(id, last);
    }
  }
  return pieces.size === selected.size ? pieces : null;
}

/** One stable collection owns removal while preview requests come and go. */
export function CitationShelf({ announce, selected, removeLabel, onRemove, style, keys, shown, focusable, outputRef,
  previewIds, searchRef, visible = true, copied = false }: {
  announce: boolean; selected: Map<string, SearchResult>;
  removeLabel: (r: SearchResult) => string; onRemove: (ids: ReadonlySet<string>) => void;
  style: StyleId | null; keys: boolean;
  shown: FormattedCitation | "failed" | "stale" | null | undefined;
  focusable: boolean; outputRef?: Ref<HTMLElement>;
  previewIds?: readonly string[]; searchRef?: RefObject<HTMLElement | null>;
  visible?: boolean; copied?: boolean;
}) {
  const fallback = useRef<HTMLElement>(null);
  const shelfRef = useRef<HTMLDivElement>(null);
  const order = useRef<string[]>([]);
  const cache = useRef<Map<string, Piece> | null>(null);
  const ready = projection(shown, previewIds, selected);
  if (ready || shown !== null) cache.current = ready;
  // Keep surviving controls and their widths while the replacement is pending.
  // These dimmed pieces are never used for clipboard output or metadata mapping.
  const pieces = ready ?? (shown === null && cache.current
    ? new Map([...cache.current].filter(([id]) => selected.has(id))) : null);
  useLayoutEffect(() => {
    if (!visible && shelfRef.current?.contains(document.activeElement)) searchRef?.current?.focus({ preventScroll: true });
  }, [visible, searchRef]);
  const retained = order.current.filter((id) => selected.has(id));
  const known = pieces ? [...pieces.keys()] : retained;
  const knownIds = new Set(known);
  order.current = [...known, ...[...selected.keys()].filter((id) => !knownIds.has(id))];
  const labels = chipLabels(order.current.map((id) => selected.get(id)!));
  const items: ChipItem[] = order.current.map((id, index) => {
    const record = selected.get(id)!;
    const piece = pieces?.get(id);
    return { id, label: record.key, title: record.key, textValue: labels[index]!, removeLabel: removeLabel(record),
      content: piece ? render(piece.content) : labels[index], before: piece ? render(piece.before) : undefined, after: piece ? render(piece.after) : undefined };
  });
  const built = shown && typeof shown === "object" ? shown : null;
  return <>
    <p className="visually-hidden" role="status" aria-label={pickerText.citationSelection} aria-live="polite" aria-atomic="true">
      {announce ? pickerText.selectionCount(selected.size) : ""}
    </p>
    {(selected.size > 0 || shown !== undefined) && <div ref={shelfRef} className="shelf" hidden={!visible}>
      <section ref={shown === undefined ? undefined : outputRef} aria-label={shown === undefined ? undefined : pickerText.preview}
        className={shown === undefined ? "shelf-controls" : "shelf-output"} data-keys={keys || undefined}
        title={built?.text} data-state={typeof shown === "string" || shown === null ? shown ?? "pending" : undefined}
        tabIndex={focusable && built ? 0 : undefined}>
        {shown !== undefined && <svg className="shelf-mark" viewBox={beak.viewBox} fill="currentColor" aria-hidden="true" focusable="false"
          preserveAspectRatio="xMaxYMid meet" dangerouslySetInnerHTML={{ __html: beak.body }} />}
        {!pieces && <>
          {shown === null && <p role="status">{pickerText.previewPending}</p>}
          {shown === "failed" && <p role="alert">{pickerText.previewError}</p>}
          {shown === "stale" && <p role="status">{pickerText.shelfStale}</p>}
          {built && <bdi dir={keys ? "ltr" : "auto"} className="preview-text" title={built.text}>{built.markup.length ? render(built.markup) : built.text}</bdi>}
        </>}
        {pieces && shown === null && <span className="visually-hidden" role="status">{pickerText.previewPending}</span>}
        <div className={pieces ? "shelf-segments preview-text" : "shelf-fallback"}>
          {selected.size > 0 && <ChipGroup label={pickerText.selectedReferences} variant="citation" dir={keys ? "ltr" : "auto"}
            items={items} fallbackFocusRef={searchRef ?? fallback} onRemove={onRemove} />}
        </div>
        {copied && <><svg className="shelf-mark shelf-close" viewBox={beak.viewBox} fill="currentColor" aria-hidden="true" focusable="false"
          dangerouslySetInnerHTML={{ __html: beak.body }} /><span className="shelf-copied">{pickerText.copied}</span></>}
        {style && provisional(style) && <span className="shelf-pill" title={pickerText.provisional}><span aria-hidden="true">{pickerText.provisionalShort}</span><span className="visually-hidden">{pickerText.provisional}</span></span>}
        {built && built.warnings.length > 0 && <span className="shelf-pill" title={built.warnings.join("\n")}>
          <span aria-hidden="true">{pickerText.warningCount(built.warnings.length)}</span><span className="visually-hidden">{built.warnings.join(" ")}</span>
        </span>}
      </section>
    </div>}
  </>;
}
