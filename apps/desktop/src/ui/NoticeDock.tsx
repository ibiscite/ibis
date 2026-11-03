import { Children, type ReactNode } from "react";
import { pickerText } from "../messages";
import "./NoticeDock.css";

/** One slot in the layout for notices that pass; nothing is drawn while it is empty. */
export function NoticeDock({ children }: { children: ReactNode }) {
  return Children.toArray(children).length ? <div className="notice-dock" role="region" aria-label={pickerText.notices}>{children}</div> : null;
}

/** A remark that needs nothing from the person. Refusals and sizes are announced; a passive one is not. */
export function QuietNotice({ children, passive = false }: { children: ReactNode; passive?: boolean }) {
  return <div className="notice-band"><span role={passive ? undefined : "status"}>{children}</span></div>;
}
