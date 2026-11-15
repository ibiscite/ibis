import type { ComponentProps, ReactNode } from "react";
import { Button } from "./Button";
import "./PopupButton.css";
import type { AccessibleName } from "./AccessibleName";
import { Icon } from "../icons";

/** Every pop-up has the same value, chevron and control geometry. */
export function PopupButton({ children, className = "", ...props }: Omit<ComponentProps<typeof Button>, "children"> & { children: ReactNode } & AccessibleName) {
  return <Button {...props} className={`ui-popup-button ${className}`}><span className="toolbar-menu-value">{children}</span>
    <Icon name="chevrons" className="popup-indicator" />
  </Button>;
}
