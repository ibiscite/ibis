import type { ComponentProps, ReactNode } from "react";
import { Button } from "./Button";
import "./PopupButton.css";
import { Icon } from "../icons";
import type { AccessibleName } from "./AccessibleName";

/** Every pop-up has the same value, chevron and control geometry. */
export function PopupButton({ children, className = "", ...props }: Omit<ComponentProps<typeof Button>, "children"> & { children: ReactNode } & AccessibleName) {
  return <Button {...props} className={`ui-popup-button ${className}`}><span className="toolbar-menu-value">{children}</span>
    <Icon name="chevrons" className="popup-indicator" />
  </Button>;
}
