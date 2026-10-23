import type React from "react";
import { memo, useCallback, useLayoutEffect, useMemo, useRef } from "react";
import type { Density, SearchResult } from "../../api";
import { rowClickAction } from "../../copyFeedback";
import { isMac } from "../../platform";
import { AddGlyph, TypeIcon } from "../../icons";
import { pickerText, rowText } from "../../messages";
import { PointerButton } from "../../ui/Button";
import { Option } from "../../ui/Option";
import { idKey } from "./choice";
import { abbreviatedAuthors, hiddenMatch, tileName } from "./rows";

/** "et al." in its own isolate, so it keeps its reading order beside a right-to-left name. */
const etAl = <bdi>{rowText.etAl}</bdi>;

type RowHandlers = {
  onPointerDown: (e: React.PointerEvent) => void;
  onClick: (index: number, record: SearchResult, e: React.MouseEvent) => void;
  onContext: (index: number, record: SearchResult, e: React.MouseEvent) => void;
  onTileClick: (index: number, record: SearchResult) => void;
};

/** The result list: one option per search result, the highlighted one active.
 * Rows are memoised; moving the highlight re-renders only the rows it leaves and reaches. */
export function ResultList({ results, active, selected, density, hoverAllowed = false, libraryOf, onMouseMove, onChoose, onToggle, onContext }: {
  results: SearchResult[];
  active: number;
  selected: Map<string, SearchResult>;
  density: Density;
  hoverAllowed?: boolean;
  /** The library a row's reference comes from, when more than one is searched; else null. */
  libraryOf: (source: string) => string | null;
  onMouseMove: React.MouseEventHandler<HTMLUListElement>;
  /** A click on a row asked to select, copy or toggle it in the citation. */
  onChoose: (index: number, record: SearchResult, action: "select" | "copy" | "toggle") => void;
  onContext?: (index: number, record: SearchResult) => void;
  /** The row's add/remove tile was clicked. */
  onToggle: (index: number, record: SearchResult) => void;
}) {
  // Where the pointer went down, so a drag that ends on a row does not copy it.
  const pressedAt = useRef<{ x: number; y: number; onControl: boolean } | null>(null);
  // Rows keep the same handlers across renders; they call the newest callbacks.
  const latest = useRef({ onChoose, onToggle, onContext });
  useLayoutEffect(() => { latest.current = { onChoose, onToggle, onContext }; });
  const onPointerDown = useCallback((e: React.PointerEvent) => {
    pressedAt.current = { x: e.clientX, y: e.clientY, onControl: e.target instanceof Element && e.target.closest("[data-pointer-only]") !== null };
  }, []);
  const onClick = useCallback((index: number, record: SearchResult, e: React.MouseEvent) => {
    const start = pressedAt.current;
    pressedAt.current = null;
    const action = rowClickAction({
      shiftKey: e.shiftKey, ctrlKey: e.ctrlKey, metaKey: e.metaKey, altKey: e.altKey, detail: e.detail,
      moved: start ? Math.hypot(e.clientX - start.x, e.clientY - start.y) : 0,
      onControl: start?.onControl || (e.target as Element).closest("button, a, input, select, textarea, [role=button]") !== null,
      selectedText: !(window.getSelection()?.isCollapsed ?? true),
    }, isMac);
    if (action === "none") return;
    latest.current.onChoose(index, record, action);
  }, []);
  const onTileClick = useCallback((index: number, record: SearchResult) => {
    // React Aria consumes the press before the row's copy action.
    pressedAt.current = null;
    latest.current.onToggle(index, record);
  }, []);
  const context = useCallback((index: number, record: SearchResult, e: React.MouseEvent) => {
    e.preventDefault();
    pressedAt.current = null;
    latest.current.onContext?.(index, record);
  }, []);
  const handlers = useMemo<RowHandlers>(() => ({ onPointerDown, onClick, onTileClick, onContext: context }), [onPointerDown, onClick, onTileClick, context]);
  return (
    <ul id="results" role="listbox" aria-label={pickerText.results} onMouseMove={onMouseMove}>
      {results.map((r, i) => (
        <ResultRow key={idKey(r.id)} record={r} index={i} active={i === active} added={selected.has(idKey(r.id))}
          density={density} hoverAllowed={hoverAllowed} library={libraryOf(r.id.source)} handlers={handlers} />
      ))}
    </ul>
  );
}

/** One search result as an option of the result list. */
export const ResultRow = memo(function ResultRow({ record: r, index: i, active, added, density, hoverAllowed, library, handlers }: {
  record: SearchResult;
  index: number;
  active: boolean;
  /** Whether the reference is in the citation being built. */
  added: boolean;
  density: Density;
  hoverAllowed: boolean;
  library: string | null;
  handlers: RowHandlers;
}) {
  const compact = density === "compact";
  // A match in the container is always named: the row may shorten the container.
  const why = hiddenMatch(r.matched);
  const itemType = density === "spacious" ? rowText.itemType(r.item_type) : null;
  return (
    <Option hoverAllowed={hoverAllowed}
      id={`result-${i}`}
      role="option"
      aria-selected={active}
      data-citation-selected={added || undefined}
      data-active={active || undefined}
      onPointerDownCapture={handlers.onPointerDown}
      onMouseDown={(e) => {
        // Double-click copies instead of selecting a word; single-press drags remain selectable.
        if (e.detail === 2 && e.button === 0 && !e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey) e.preventDefault();
      }}
      onContextMenu={(e) => handlers.onContext(i, r, e)}
      onClick={(e) => handlers.onClick(i, r, e)}
    >
      {/* The leading tile: the type icon, which gives way to the add
          (or remove) control on hover and on the active row. An option's
          children are presentational (ARIA) and must not be focusable, so
          the control is for the pointer only: no Tab stop, no role, its
          name in the tooltip. Keyboards use Shift+Enter. */}
      <span className="result-tile">
        <TypeIcon itemType={r.item_type} />
        <PointerButton className="tile-button" hoverAllowed={hoverAllowed}
          title={added ? pickerText.removeNamed(tileName(r)) : pickerText.addNamed(tileName(r))}
          onPress={() => handlers.onTileClick(i, r)}>
          <AddGlyph added={added} />
        </PointerButton>
      </span>
      <span className="result-body">
        <span className="result-line">
          <strong className="result-title">{r.title}</strong>
          {compact && r.first_author && <span className="result-first-author" dir="auto">{r.first_author}{r.author_count > 1 && <> {etAl}</>}</span>}
        </span>
        <span className="result-meta">
          {/* Each clipping part takes its own text direction, so Latin text in a
              right-to-left row is cut at its end. The separator is its own part. */}
          <span className="result-byline">
            <span className="result-authors" dir="auto">
              {abbreviatedAuthors(r) ? <><span aria-hidden="true">{r.first_author} {etAl}</span><span className="visually-hidden">{r.authors}</span></> : r.authors}
            </span>
            {!compact && r.container && r.authors && <span className="result-separator" aria-hidden="true">{" · "}</span>}
            {!compact && r.container && <span className="result-container" dir="auto">{r.container}</span>}
          </span>
          {itemType && <span className="result-type">{itemType}</span>}
          <code className="result-key" dir="ltr" title={r.key}>{r.key}</code>
          {library && <span className="result-library">{library}</span>}
        </span>
        {why && <span className="result-why">{pickerText.matchedIn(why)}</span>}
        {/* The type in words where the row does not show it; the tile's icon is decorative. */}
        {!itemType && rowText.itemType(r.item_type) && <span className="visually-hidden">{rowText.itemType(r.item_type)}</span>}
        {added && <span className="visually-hidden">{pickerText.addedToCitation}</span>}
        {compact && <span className="visually-hidden">{r.key}</span>}
      </span>
      <span className="result-year">{r.year ?? pickerText.noDate}</span>
    </Option>
  );
});
