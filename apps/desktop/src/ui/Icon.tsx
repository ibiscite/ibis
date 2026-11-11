import { iconPaths, iconStroke, iconViewBox } from "../icon-paths";
import "./Icon.css";

export type IconName = keyof typeof iconPaths;
export type IconSize = keyof typeof iconStroke;

/** Lucide's 24 unit drawing, with a 1.5 px stroke at either supported size. */
export function Icon({ name, size = "control", className = "" }: { name: IconName; size?: IconSize; className?: string }) {
  return <svg className={`ui-icon ${className}`} data-size={size} aria-hidden="true" focusable="false" viewBox={iconViewBox} fill="none"
    stroke="currentColor" strokeWidth={iconStroke[size]} strokeLinecap="round" strokeLinejoin="round"><path d={iconPaths[name]} pathLength="1" /></svg>;
}
