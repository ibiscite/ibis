import { useLayoutEffect, useRef, type KeyboardEventHandler, type ReactNode, type Ref } from "react";
import { IconButton } from "./IconButton";
import { useScrollEdges } from "./ScrollArea";
import "./Inspector.css";

/** One thing's details, in a panel beside the list. Everything around it stays
 * live, so it is a region and never a dialog. The column keeps its controls in
 * place; whatever grows with the output, and later a source's pages, belongs in
 * `document`, under it or, in a panel wide enough, beside it. */
export function Inspector({ ref, id, label, heading, byline, closeLabel, document: page, readOnly = false, children, onClose, onKeyDown }: {
  ref?: Ref<HTMLElement>; id?: string; label: string; heading: string; closeLabel: string;
  /** One quiet line under the heading: who and when, said once. */
  byline?: string;
  /** Nothing to edit, as for warnings: the column itself takes the keys, so they can scroll it. */
  readOnly?: boolean;
  document?: ReactNode; children: ReactNode; onClose: () => void;
  onKeyDown?: KeyboardEventHandler<HTMLElement>;
}) {
  const column = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  useScrollEdges(body);
  // With nothing to edit, the column itself takes the keys, so they can scroll it.
  // biome-ignore lint/correctness/useExhaustiveDependencies: focus is taken once, on opening.
  useLayoutEffect(() => { if (readOnly) column.current?.focus({ preventScroll: true }); }, []);
  return <section ref={ref} id={id} className="inspector" aria-label={label} onKeyDown={onKeyDown}>
    <div ref={body} className="inspector-body scroll-area">
      <div ref={column} className="inspector-column" tabIndex={readOnly ? 0 : undefined}>
        <header className="inspector-heading"><h2><bdi dir="auto">{heading}</bdi></h2><IconButton aria-label={closeLabel} onPress={onClose} /></header>
        {byline && <p className="inspector-byline"><bdi dir="auto">{byline}</bdi></p>}
        {children}
      </div>
      {page && <div className="inspector-document">{page}</div>}
    </div>
  </section>;
}
