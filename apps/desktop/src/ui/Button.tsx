import type { ComponentPropsWithRef } from "react";
import { Button as AriaButton, ToggleButton as AriaToggleButton, type ButtonProps, type ToggleButtonProps } from "react-aria-components";
import "./Button.css";

type Look = Pick<ComponentPropsWithRef<"button">, "ref" | "title" | "aria-keyshortcuts" | "aria-hidden" | "inert"> & {
  variant?: "primary" | "secondary" | "quiet" | "icon"; size?: "default" | "small"; className?: string;
};

/** Shared pointer, keyboard, touch and disabled states; callers own the action. */
export function Button({ variant = "secondary", size = "default", className = "", ...props }: Omit<ButtonProps, "className"> & Look) {
  return <AriaButton {...props} className={`ui-button ${className}`} data-variant={variant} data-size={size}
    render={props.render ?? ((native) => <button {...native as ComponentPropsWithRef<"button">} title={props.title} aria-keyshortcuts={props["aria-keyshortcuts"]} aria-hidden={props["aria-hidden"]} tabIndex={props.excludeFromTabOrder ? -1 : native.tabIndex} />)} />;
}

export function ToggleButton({ variant = "secondary", size = "default", className = "", ...props }: Omit<ToggleButtonProps, "className"> & Look) {
  return <AriaToggleButton {...props} className={`ui-button ${className}`} data-variant={variant} data-size={size}
    render={props.render ?? ((native) => <button {...native as ComponentPropsWithRef<"button">} title={props.title} aria-keyshortcuts={props["aria-keyshortcuts"]} aria-hidden={props["aria-hidden"]} tabIndex={props.excludeFromTabOrder ? -1 : native.tabIndex} />)} />;
}
