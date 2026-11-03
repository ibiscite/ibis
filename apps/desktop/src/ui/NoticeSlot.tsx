import { Children, type ReactNode } from "react";
import { pickerText } from "../messages";
import "./NoticeSlot.css";

/** A window's one place for what passes; nothing is drawn while it is empty. */
export function NoticeSlot({ children }: { children: ReactNode }) {
  return Children.toArray(children).length ? <div className="notice-slot" role="region" aria-label={pickerText.notices}>{children}</div> : null;
}

/** A remark that needs nothing from the person. Refusals and sizes are announced; a passive one is not. */
export function QuietNotice({ children, passive = false, role = "status" }: { children: ReactNode; passive?: boolean; role?: "status" | "alert" }) {
  return <div className="notice-band"><span role={passive ? undefined : role}>{children}</span></div>;
}
