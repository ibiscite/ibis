import { useEffect, useId, useLayoutEffect, useRef, type ReactNode } from "react";
import { useLayer } from "./layers";
import { Button } from "./Button";
import { ViewFrame } from "./ViewFrame";
import { useScrollEdges } from "./ScrollArea";
import "./Layout.css";
import "./Sheet.css";

/** One of a sheet's footer actions. The primary one runs on Enter too, unless a control in the body has the keyboard. */
export type SheetAction = { label: string; onPress: () => void; disabled?: boolean; primary?: boolean };
const footerAction = (action: SheetAction) => <Button key={action.label} variant={action.primary ? "action" : "quiet"} size="small"
  label={action.label} keys={action.primary ? "Enter" : undefined} isDisabled={action.disabled} onPress={action.onPress} />;

const FOCUSABLE = "button:not([disabled]):not([tabindex='-1']), input:not([disabled]), [tabindex='0']";

/** A sheet: a view of the window (ui/ViewFrame), as the palette is. Its name in
 * the header's chip, one body that alone scrolls, and the footer as its action
 * bar in the picker footer's words: leading actions, trailing actions with the
 * primary last (with ↵), then Back with Esc, each plain text as the footer's are. Esc, a hide or Back close it; Tab stays inside, and a control that
 * disappears or is disabled hands the keyboard back to the sheet, so no key acts
 * behind it. */
export function Sheet({ title, closeLabel, description, actions, leadingActions, busy, onClose, children }: {
  title: string; closeLabel: string;
  /** One quiet sentence at the top of the body that says what the sheet is for; it describes the dialog. */
  description?: ReactNode;
  /** Actions on the trailing edge of the footer, before Back; the primary goes last. */
  actions?: readonly SheetAction[];
  /** Actions on the leading edge of the footer. */
  leadingActions?: readonly SheetAction[];
  busy?: boolean;
  onClose: () => void;
  children: ReactNode;
}) {
  const dialog = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const titleId = useId(), aboutId = useId();
  useScrollEdges(body);
  useEffect(() => { dialog.current?.focus(); }, []);
  useLayer("sheet", true, () => onClose());
  // Any render can remove or disable the focused control.
  useLayoutEffect(() => {
    const focused = document.activeElement;
    if (!dialog.current?.contains(focused) || (focused as HTMLButtonElement).disabled) dialog.current?.focus();
  });
  return <ViewFrame title={title} titleId={titleId} frameRef={dialog} back={{ label: closeLabel, onBack: onClose }}
    role="dialog" aria-modal="true" aria-describedby={description ? aboutId : undefined}
    // Clicking plain text inside keeps focus here, so Escape still closes it.
    tabIndex={-1} actions={actions?.length ? actions.map(footerAction) : undefined} leadingActions={leadingActions?.length ? leadingActions.map(footerAction) : undefined}
    onKeyDown={(event) => {
      if (event.key === "Enter" && (event.target === dialog.current || event.target === body.current)) {
        const primary = actions?.find((action) => action.primary);
        if (primary && !primary.disabled && !event.repeat) { event.preventDefault(); primary.onPress(); }
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = [...(dialog.current?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? [])];
      const at = focusable.indexOf(document.activeElement as HTMLElement);
      event.preventDefault();
      focusable[at < 0 ? event.shiftKey ? focusable.length - 1 : 0 : (at + (event.shiftKey ? -1 : 1) + focusable.length) % focusable.length]?.focus();
    }}>
    <div ref={body} className="ui-sheet-body scroll-area" tabIndex={0} role="group" aria-labelledby={titleId} aria-busy={busy || undefined}>
      <div className="ui-sheet-content">
        {description && <p id={aboutId} className="ui-sheet-lead">{description}</p>}
        {children}
      </div>
    </div>
  </ViewFrame>;
}

/** What a sheet will copy or save, kept as written and selectable, on a card. */
export function SheetOutput({ children }: { children: ReactNode }) {
  return <div className="ui-group"><p className="ui-sheet-output" dir="auto">{children}</p></div>;
}

/** A line reserved under a sheet's content for a spinner or a problem, so nothing above it moves. */
export function SheetStatus({ children }: { children?: ReactNode }) {
  return <div className="ui-sheet-status">{children}</div>;
}
