import { Children, type ReactNode } from "react";
import { say } from "../i18n";
import { useFrameSlot } from "./Frame";
import { useTimeout } from "./timers";
import "./NoticeSlot.css";

/** A window's one place for what passes; nothing is drawn while it is empty. */
export function NoticeSlot({ children }: { children: ReactNode }) {
  const present = Children.toArray(children).length > 0;
  useFrameSlot("notice", present);
  return present ? <div className="notice-slot" role="region" aria-label={say("picker-notices")}>{children}</div> : null;
}

/** A remark that needs nothing from the person. Refusals and sizes are announced; a
 * passive one is not. Its 8 s run only while it is shown, so none passes unseen. */
export function QuietNotice({ children, passive = false, role = "status", onExpire, paused = false }: {
  children: ReactNode; passive?: boolean; role?: "status" | "alert"; onExpire?: () => void; paused?: boolean;
}) {
  // A new remark starts its own time.
  useTimeout(() => onExpire?.(), onExpire && !paused ? 8000 : null, children);
  return <div className="notice-band"><span role={passive ? undefined : role}>{children}</span></div>;
}
