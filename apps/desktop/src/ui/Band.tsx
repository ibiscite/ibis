import type { ComponentPropsWithRef, MouseEvent, ReactNode } from "react";
import { DRAG_REGION, startsWindowDrag } from "./windowDrag";
import "./Band.css";

/** [C3] One row of the window on the shared columns: an icon column, a label
 * column and trailing controls, inside the window inset. The search bar, a
 * view's header and footer, the notice line, list headings and list rows are
 * bands, so a glyph and the words after it line up by construction. A `fill`
 * band is a list row: its fill starts on the inset and its words end on its padding. */
export function Band({ as: Tag = "div", className, icon, trailing, fill, drag, onDragReset, children, ...rest }: {
  as?: "div" | "header" | "footer" | "section" | "li";
  /** The lead column's content; null keeps the column empty (a row without a
   * glyph). Left out, the band has no lead column and its words start on the
   * leading edge, as headings, notices and status do. */
  icon?: ReactNode;
  /** Controls that end on the window inset. */
  trailing?: ReactNode;
  fill?: boolean;
  /** The band moves the window: its own box and its slots' (the glyph in the
   * lead column, the room between trailing controls) are drag regions, so
   * pressing anywhere in it but a control or a field moves the window. */
  drag?: boolean;
  /** Double-clicking where the band drags: never the system's zoom. */
  onDragReset?: () => void;
  children?: ReactNode;
} & ComponentPropsWithRef<"div">) {
  const region = drag ? { [DRAG_REGION]: "" } : undefined;
  // React's listener runs before the drag script's on the document, which would zoom the window.
  const twice = (event: MouseEvent<HTMLElement>) => {
    const reset = drag && event.button === 0 && event.detail === 2 && startsWindowDrag(event.target);
    if (reset) event.nativeEvent.stopImmediatePropagation();
    return reset;
  };
  const press = drag ? {
    onMouseDown: (event: MouseEvent<HTMLElement>) => { twice(event); rest.onMouseDown?.(event as MouseEvent<HTMLDivElement>); },
    onMouseUp: (event: MouseEvent<HTMLElement>) => { if (twice(event)) onDragReset?.(); rest.onMouseUp?.(event as MouseEvent<HTMLDivElement>); },
  } : undefined;
  return <Tag className={className ? `ui-band ${className}` : "ui-band"} data-band={icon !== undefined ? "title" : "leading"} data-fill={fill || undefined} {...rest as object} {...region} {...press}>
    {icon !== undefined && <span className="ui-band-icon" data-band-slot="lead" {...region}>{icon}</span>}
    {children}
    {trailing !== undefined && <span className="ui-band-trailing" data-band-slot="trailing" {...region}>{trailing}</span>}
  </Tag>;
}
