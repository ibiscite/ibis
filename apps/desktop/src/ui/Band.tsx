import type { ComponentPropsWithRef, ReactNode } from "react";
import "./Band.css";

/** [C3] One row of the window on the shared columns: an icon column, a label
 * column and trailing controls, inside the window inset. The search bar, a
 * view's header and footer, the notice line, list headings and list rows are
 * bands, so a glyph and the words after it line up by construction. A `fill`
 * band is a list row: its fill starts on the inset and its words end on its padding. */
export function Band({ as: Tag = "div", className, icon, trailing, fill, children, ...rest }: {
  as?: "div" | "header" | "footer" | "section" | "li";
  /** The icon column's content; empty, the column is still kept. */
  icon?: ReactNode;
  /** Controls that end on the window inset. */
  trailing?: ReactNode;
  fill?: boolean;
  children?: ReactNode;
} & ComponentPropsWithRef<"div">) {
  return <Tag className={className ? `ui-band ${className}` : "ui-band"} data-fill={fill || undefined} {...rest as object}>
    <span className="ui-band-icon">{icon}</span>
    {children}
    {trailing !== undefined && <span className="ui-band-trailing">{trailing}</span>}
  </Tag>;
}
