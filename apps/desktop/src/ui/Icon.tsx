import { art } from "../icon-art";
import "./Icon.css";

export type IconName = keyof typeof art.body;
export type IconSize = "control" | "tile";
/** Glyphs that point back or forward in reading order turn around in a
 * right-to-left window. Left and right arrows name a visual side and stay. */
const MIRRORED: ReadonlySet<IconName> = new Set(["back"]);

/** One glyph, named by what it means, from the vendored Phosphor Regular set
 * (brand/icons): drawn exactly as upstream drew it, at 16 px in controls and
 * menus and 20 px in a list's lead column. */
export function Icon({ name, size = "control", className = "" }: { name: IconName; size?: IconSize; className?: string }) {
  return <svg className={`ui-icon ${className}`} data-size={size} data-mirrored={MIRRORED.has(name) || undefined} aria-hidden="true" focusable="false" viewBox={art.viewBox}
    fill="currentColor" dangerouslySetInnerHTML={{ __html: art.body[name] }} />;
}
