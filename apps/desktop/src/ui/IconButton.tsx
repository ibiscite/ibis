import type { ComponentProps } from "react";
import { Icon, type IconName } from "../icons";
import { Button, ToggleButton } from "./Button";
import "./IconButton.css";

type Props = Omit<ComponentProps<typeof Button>, "variant" | "children" | "icon" | "label" | "keys" | "size"> & {
  icon?: IconName; size?: "regular" | "large"; isSelected?: boolean; onChange?: (selected: boolean) => void;
};

/** One square, one glyph, one state set. Selection never changes its drawing. */
export function IconButton({ icon = "close", size = "regular", isSelected, onChange, className = "", ...props }: Props) {
  const shared = { ...props, variant: "icon" as const, size: size === "large" ? "default" as const : "small" as const,
    className: `ui-icon-button ${className}` };
  return isSelected === undefined ? <Button {...shared}><Icon name={icon} /></Button>
    // React Aria types the toggle's capture events as div events; our renderer
    // in Button always exposes the same native button for both controls.
    : <ToggleButton {...shared as ComponentProps<typeof ToggleButton>} isSelected={isSelected} onChange={onChange}><Icon name={icon} /></ToggleButton>;
}
