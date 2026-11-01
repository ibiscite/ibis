import type { ComponentPropsWithRef, ReactNode } from "react";
import { mergeProps, useHover, usePress } from "react-aria";
import { Button as AriaButton, ToggleButton as AriaToggleButton, type ButtonProps, type ToggleButtonProps } from "react-aria-components";
import "./Button.css";
import { ariaKeys } from "../keys";
import { currentPlatform } from "../platform";
import { KeyHint } from "./KeyCaps";

type Look = Pick<ComponentPropsWithRef<"button">, "ref" | "title" | "aria-busy" | "aria-keyshortcuts" | "aria-hidden" | "inert"> & {
  variant?: "primary" | "secondary" | "quiet" | "icon"; size?: "default" | "small"; className?: string;
  label?: string; keys?: string; icon?: ReactNode;
};

/** Shared pointer, keyboard, touch and disabled states; callers own the action. */
export function Button({ variant = "secondary", size = "default", className = "", label, keys, icon, children, ...props }: Omit<ButtonProps, "className"> & Look) {
  const shortcut = keys ? ariaKeys(keys, currentPlatform() === "macos") : props["aria-keyshortcuts"];
  return <AriaButton {...props} aria-label={props["aria-label"] ?? label} className={`ui-button ${className}`} data-variant={variant} data-size={size}
    render={props.render ?? ((native) => <button {...native as ComponentPropsWithRef<"button">} title={props.title} aria-busy={props["aria-busy"]} aria-keyshortcuts={shortcut} aria-hidden={props["aria-hidden"]} tabIndex={props.excludeFromTabOrder ? -1 : native.tabIndex} />)}>
    {(values) => <>{icon && <span className="ui-button-icon">{icon}</span>}
      {label ? keys ? <KeyHint label={label} keys={keys} primary={variant === "primary"} hidden echo /> : <bdi>{label}</bdi> : typeof children === "function" ? children(values) : children}</>}
  </AriaButton>;
}

export function ToggleButton({ variant = "secondary", size = "default", className = "", ...props }: Omit<ToggleButtonProps, "className"> & Look) {
  return <AriaToggleButton {...props} className={`ui-button ${className}`} data-variant={variant} data-size={size}
    render={props.render ?? ((native) => <button {...native as ComponentPropsWithRef<"button">} title={props.title} aria-busy={props["aria-busy"]} aria-keyshortcuts={props["aria-keyshortcuts"]} aria-hidden={props["aria-hidden"]} tabIndex={props.excludeFromTabOrder ? -1 : native.tabIndex} />)} />;
}

/** A presentational option child: pointer states without a role or Tab stop. */
export function PointerButton({ onPress, hoverAllowed, className = "", ...props }: Pick<ComponentPropsWithRef<"span">, "children" | "title" | "className"> & { onPress: () => void; hoverAllowed: boolean }) {
  const { pressProps, isPressed } = usePress({ onPress, preventFocusOnPress: true });
  const { hoverProps, isHovered } = useHover({});
  return <span {...props} {...mergeProps(pressProps, hoverProps)} className={`ui-button ${className}`} aria-hidden="true"
    data-variant="icon" data-pointer-only data-hovered={hoverAllowed && isHovered || undefined} data-pressed={isPressed || undefined} />;
}
