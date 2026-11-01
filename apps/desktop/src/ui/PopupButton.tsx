import type { ComponentProps, ReactNode } from "react";
import { Button } from "./Button";
import "./PopupButton.css";

/** Every pop-up has the same value, chevron and control geometry. */
export function PopupButton({ children, className = "", ...props }: Omit<ComponentProps<typeof Button>, "children"> & { children: ReactNode }) {
  return <Button {...props} className={`ui-popup-button ${className}`}><span className="toolbar-menu-value">{children}</span>
    <svg className="popup-indicator popup-indicator-macos" aria-hidden="true" focusable="false" viewBox="0 0 8 12" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round"><path d="m1 4 3-3 3 3m-6 4 3 3 3-3" /></svg>
  </Button>;
}
