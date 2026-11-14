import { useId, useLayoutEffect, useRef, type ReactNode, type RefObject } from "react";
import { Button } from "./Button";
import { flushSync } from "react-dom";
import { useLayer } from "./layers";
import { ViewFrame } from "./ViewFrame";
import { useScrollEdges } from "./ScrollArea";
import "./Layout.css";
import "./Sheet.css";
import { tabStops } from "./focus";

/** One of a sheet's footer actions. The primary one runs on Enter too, unless a control in the body has the keyboard. */
export type SheetAction = { label: string; onPress: () => void; disabled?: boolean; primary?: boolean };
const footerAction = (action: SheetAction) => <Button key={action.label} variant={action.primary ? "action" : "quiet"} size="small"
  label={action.label} keys={action.primary ? "Enter" : undefined} isDisabled={action.disabled} onPress={action.onPress} />;

/** A sheet's owner can retain its place alongside its draft, without keeping I/O mounted. */
export type SheetMemory = { focus: string | null; selection?: [number | null, number | null]; scroll: number };
const focusName = (node: HTMLElement) => node.getAttribute("aria-label")
  ?? node.getAttribute("aria-labelledby")?.split(" ").map((id) => document.getElementById(id)?.textContent ?? "").join(" ")
  ?? (node instanceof HTMLInputElement ? node.labels?.[0]?.textContent : node.textContent) ?? "";

/** A sheet: a view of the window (ui/ViewFrame), as the palette is. Its name in
 * plain in the header, one body that alone scrolls, and the footer as its action
 * bar in the picker footer's words: leading actions, trailing actions with the
 * primary last (with ↵), then Back with Esc, each plain text as the footer's are. Esc, a hide or Back close it; Tab stays inside, and a control that
 * disappears or is disabled hands the keyboard back to the sheet, so no key acts
 * behind it. */
export function Sheet({ title, closeLabel, description, actions, leadingActions, busy, onClose, children, memory }: {
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
  memory?: RefObject<SheetMemory | null>;
}) {
  const dialog = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const titleId = useId(), aboutId = useId();
  useScrollEdges(body);
  useLayoutEffect(() => { dialog.current?.focus(); }, []);
  // Esc and Back go back (ui/layers). A command can open native UI next, so the sheet is already gone then.
  const layer = useLayer("sheet", true, () => { flushSync(dismiss); });
  function dismiss() {
    if (memory) {
      const focused = document.activeElement;
      memory.current = { focus: focused instanceof HTMLElement && focused !== dialog.current && dialog.current?.contains(focused) ? focusName(focused) : null,
        selection: focused instanceof HTMLInputElement ? [focused.selectionStart, focused.selectionEnd] : undefined,
        scroll: body.current?.scrollTop ?? 0 };
    }
    layer.leave();
    onClose();
  }
  // Any render can remove or disable the focused control.
  useLayoutEffect(() => {
    const focused = document.activeElement;
    if (!dialog.current?.contains(focused) || (focused as HTMLButtonElement).disabled) dialog.current?.focus();
    const saved = memory?.current;
    if (saved) {
      const target = (dialog.current ? tabStops(dialog.current) : []).find((node) => focusName(node) === saved.focus);
      if (saved.focus !== null && !target) return; // The draft's controls may follow an asynchronous read.
      target?.focus({ preventScroll: true });
      if (target instanceof HTMLInputElement && saved.selection && saved.selection[0] !== null && saved.selection[1] !== null) target.setSelectionRange(saved.selection[0], saved.selection[1]);
      if (body.current) body.current.scrollTop = saved.scroll;
      memory!.current = null;
    }
  });
  return <ViewFrame title={title} titleId={titleId} frameRef={dialog} back={{ label: closeLabel, onBack: dismiss }} layer={layer}
    role="dialog" aria-modal="true" aria-describedby={description ? aboutId : undefined}
    // Clicking plain text inside keeps focus here, so Escape still closes it.
    tabIndex={-1} actions={actions?.length ? actions.map(footerAction) : undefined} leadingActions={leadingActions?.length ? leadingActions.map(footerAction) : undefined}
    onKeyDown={(event) => {
      if (event.key === "Enter" && (event.target === dialog.current || event.target === body.current)) {
        const primary = actions?.find((action) => action.primary);
        if (primary && !primary.disabled && !event.repeat) { event.preventDefault(); primary.onPress(); }
        return;
      }
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
  return <div className="ui-group"><div className="ui-sheet-output" dir="auto">{children}</div></div>;
}

/** A line reserved under a sheet's content for a spinner or a problem, so nothing above it moves. */
export function SheetStatus({ children }: { children?: ReactNode }) {
  return <div className="ui-sheet-status">{children}</div>;
}
