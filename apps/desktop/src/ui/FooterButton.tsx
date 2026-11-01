import type { ComponentProps, ReactNode } from "react";
import { ariaKeys } from "../keys";
import { currentPlatform } from "../platform";
import { Button } from "./Button";
import { KeyHint } from "./KeyCaps";
import "./FooterButton.css";

/** Source and copy actions share geometry; keys are plain text within the fill. */
export function FooterButton({ label, keys, icon, className = "", variant = "secondary", ...props }: Omit<ComponentProps<typeof Button>, "children"> & {
  label: string; keys?: string; icon?: ReactNode;
}) {
  return <Button {...props} variant={variant} size="default" className={`ui-footer-button ${className}`} aria-label={props["aria-label"] ?? label}
    aria-keyshortcuts={keys ? ariaKeys(keys, currentPlatform() === "macos") : undefined}>
    {icon}{keys ? <KeyHint label={label} keys={keys} primary={variant === "primary"} hidden echo /> : <bdi>{label}</bdi>}
  </Button>;
}
