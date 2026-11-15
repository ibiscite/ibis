import type { ComponentPropsWithRef, ReactNode } from "react";
import "./Band.css";

/** [C3] One row of the window on the shared columns: an icon column, a label
 * column and trailing controls, inside the window inset. The search bar, a
 * view's header and footer, the notice line, list headings and list rows are
 * bands, so a glyph and the words after it line up by construction. A `fill`
 * band is a list row: its fill starts on the inset and its words end on its padding. */
export function Band({ as: Tag = "div", className, icon, trailing, fill, children, ...rest }: {
  as?: "div" | "header" | "footer" | "section" | "li";
  /** The lead column's content; null keeps the column empty (a row without a
   * glyph). Left out, the band has no lead column and its words start on the
   * leading edge, as headings, notices and status do. */
  icon?: ReactNode;
  /** Controls that end on the window inset. */
  trailing?: ReactNode;
  fill?: boolean;
  children?: ReactNode;
} & ComponentPropsWithRef<"div">) {
  return <Tag className={className ? `ui-band ${className}` : "ui-band"} data-band={icon !== undefined ? "title" : "leading"} data-fill={fill || undefined} {...rest as object}>
    {icon !== undefined && <span className="ui-band-icon" data-band-slot="lead">{icon}</span>}
    {children}
    {trailing !== undefined && <span className="ui-band-trailing" data-band-slot="trailing">{trailing}</span>}
  </Tag>;
}
