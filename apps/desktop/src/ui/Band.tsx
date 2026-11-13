import type { HTMLAttributes, ReactNode } from "react";
import "./Band.css";

/** [C3] One row of the window on the shared columns: an icon column, a label
 * column and trailing controls, inside the window inset. The search bar, a
 * view's header and footer and the notice line are bands, so a glyph and the
 * words after it line up with the result rows by construction. */
export function Band({ as: Tag = "div", className, icon, trailing, children, ...rest }: {
  as?: "div" | "header" | "footer" | "section";
  /** The icon column's content; empty, the column is still kept. */
  icon?: ReactNode;
  /** Controls that end on the window inset. */
  trailing?: ReactNode;
  children?: ReactNode;
} & HTMLAttributes<HTMLElement>) {
  return <Tag className={className ? `ui-band ${className}` : "ui-band"} {...rest}>
    <span className="ui-band-icon">{icon}</span>
    {children}
    {trailing !== undefined && <span className="ui-band-trailing">{trailing}</span>}
  </Tag>;
}
