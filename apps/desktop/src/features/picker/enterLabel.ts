import { writesEntries, type OutputId } from "../../format/outputs";
import { say } from "../../i18n";

/** Whether Enter pastes rather than copies. The bar and the keys sheet both ask this. */
export function enterPastes(app: string | null, canPaste: boolean, stayOpen: boolean): app is string {
  return canPaste && app !== null && app !== "" && !stayOpen;
}

/** Where the citation goes, for the header and the keys sheet: pasted, or to the
 * editor that asked for it. The bar's action names the destination itself. */
export function citationDelivery(app: string | null, canPaste: boolean, stayOpen: boolean, editorFormat: string | null) {
  return { pastes: !editorFormat && enterPastes(app, canPaste, stayOpen), editorFormat };
}

/** Everything Enter can do, each named by the bar's one action:
 * - nothing: no citation yet, or one that is out of date or failed;
 * - send: an editor asked for the citation, and takes it;
 * - open: Find mode opens the highlighted reference's PDF or shows it in
 *   Zotero; with no result to open, Enter does nothing there;
 * - deciding: this opening's practice offer has not answered yet, so Enter
 *   waits for it; the bar keeps the action's room but names nothing, so its
 *   words never promise what Enter will not do;
 * - practice: the first screen's paste into Ibis itself, to try it;
 * - change: the citation last pasted in this app is replaced;
 * - paste, copy: delivered to the app, or to the clipboard.
 * A refused citation keeps its outcome's words with the action off (`off`):
 * Enter then delivers nothing and the bar says why. A paste that needs the
 * system's permission is still a paste: Enter tries it, the copy it falls back
 * to says what to allow, and back from Settings, Enter pastes. */
export type EnterOutcome = "nothing" | "send" | "open" | "deciding" | "practice" | "change" | "paste" | "copy";

export type EnterInputs = {
  /** References Enter would deliver. */
  count: number;
  /** The prepared citation: ready, or out of date or failed, which Enter cannot deliver. */
  preview: "ready" | "stale" | "failed";
  editor: boolean; findOpen: boolean; practising: boolean; changing: boolean;
  /** Find mode has a highlighted result to open. */
  openable: boolean;
  /** Whether this opening's practice offer is still being read: Enter waits on it. */
  deciding: boolean;
  format: OutputId; app: string | null; canPaste: boolean;
  /** A kept-open window or a citation with warnings stays, so Enter copies. */
  stayOpen: boolean;
  /** The person's order is kept in a style that would sort it. */
  ownOrder: boolean;
  /** Why the citation is refused, when it is. */
  refusal: string | null;
};

export function enterOutcome(i: EnterInputs): EnterOutcome {
  if (i.count === 0 || i.preview !== "ready") return "nothing";
  if (i.editor) return "send";
  if (i.findOpen) return i.openable ? "open" : "nothing";
  if (i.practising) return "practice";
  if (i.deciding) return "deciding";
  if (i.changing) return "change";
  return enterPastes(i.app, i.canPaste, i.stayOpen) ? "paste" : "copy";
}

/** The bar's one action: what Enter does, in words, with its destination named
 * once and the format never (the header names it). Null when Enter does nothing.
 * While Enter's outcome is being decided the action is `hidden`: it keeps the
 * delivery's room, so nothing moves when the answer arrives, and says nothing. */
export function enterAction(i: EnterInputs): { outcome: EnterOutcome; label: string | null; off: boolean; hidden: boolean } {
  const outcome = enterOutcome(i);
  if (outcome === "deciding") return { ...enterAction({ ...i, deciding: false }), outcome, hidden: true };
  const entries = writesEntries(i.format);
  const order = i.ownOrder && i.count > 1 ? ` · ${say("picker-your-order")}` : "";
  const label = outcome === "nothing" ? null
    : outcome === "send" ? say("picker-bar-send")
    : outcome === "open" ? say("picker-find-open")
    : outcome === "practice" ? say("picker-bar-practice")
    : outcome === "change" ? say("picker-bar-change") + order
    : outcome === "paste" ? (entries ? say("picker-bar-paste-entries", { count: i.count, app: i.app ?? "" }) : say("picker-bar-paste", { app: i.app ?? "" })) + order
    : (entries ? say("picker-bar-copy-entries", { count: i.count }) : say("picker-bar-copy", { count: i.count })) + order;
  return { outcome, label, off: outcome !== "nothing" && i.refusal !== null, hidden: false };
}
