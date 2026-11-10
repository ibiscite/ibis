import type { ReactNode, RefObject } from "react";
import { OverlayArrow, Popover as AriaPopover } from "react-aria-components";
import "./Overlay.css";
import "./Popover.css";

/** A small window of details anchored to the thing it is about, in the shared
 * overlay material, growing from it. It is not modal: everything around it stays
 * live, and the picker's layers own Esc, so the caller closes it. Its dialog role
 * is plain: React Aria's Dialog would hold focus inside it. */
export function Popover({ triggerRef, label, placement = "top", className = "", children }: {
  triggerRef: RefObject<Element | null>; label: string; placement?: "top" | "bottom"; className?: string; children: ReactNode;
}) {
  return <AriaPopover triggerRef={triggerRef} isOpen isNonModal isKeyboardDismissDisabled placement={placement} offset={10} containerPadding={8}
    shouldFlip className={`ui-popover ui-overlay ${className}`}>
    <OverlayArrow className="ui-popover-arrow"><svg width={14} height={7} viewBox="0 0 14 7" aria-hidden="true"><path d="M0 0 L7 7 L14 0" /></svg></OverlayArrow>
    <div role="dialog" aria-label={label} className="ui-popover-dialog">{children}</div>
  </AriaPopover>;
}
