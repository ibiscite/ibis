import { art } from "../icon-art";
import "./Icon.css";

export type IconName = keyof typeof art.body;
export type IconSize = "control" | "tile";

/** One glyph, named by what it means, from the vendored Phosphor Regular set
 * (brand/icons): drawn exactly as upstream drew it, at 16 px in controls and
 * menus and 20 px in a list's lead column. */
export function Icon({ name, size = "control", className = "" }: { name: IconName; size?: IconSize; className?: string }) {
  return <svg className={`ui-icon ${className}`} data-size={size} aria-hidden="true" focusable="false" viewBox={art.viewBox}
    fill="currentColor" dangerouslySetInnerHTML={{ __html: art.body[name] }} />;
}
