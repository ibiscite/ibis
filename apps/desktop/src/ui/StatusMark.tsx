import { Icon } from "../icons";
import "./StatusMark.css";

export type StatusKind = "ok" | "warning" | "error";
/** Colour belongs to a redundant dot or symbol, never the adjacent words. */
export function StatusMark({ kind, presentation = "symbol" }: { kind: StatusKind; presentation?: "dot" | "symbol" | "wash" }) {
  return <span className="ui-status-mark" data-status={kind} data-presentation={presentation} aria-hidden="true">
    {presentation !== "dot" && <Icon name={kind === "ok" ? "check" : kind} />}
  </span>;
}
