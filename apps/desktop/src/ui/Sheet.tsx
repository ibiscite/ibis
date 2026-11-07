import { useEffect, useId, useLayoutEffect, useRef, type ReactNode } from "react";
import { IconButton } from "./IconButton";
import { useLayer } from "./layers";
import { useScrollEdges } from "./ScrollArea";
import "./Overlay.css";
import "./Sheet.css";

const FOCUSABLE = "button:not([disabled]), input:not([disabled]), [tabindex='0']";

/** A modal sheet over a dimmed picker: a title with ×, one padded body that alone
 * scrolls, and an optional row of actions under it. Esc, a hide, a click on the
 * backdrop or × close it; Tab stays inside, and a control that disappears or is
 * disabled hands the keyboard back to the sheet, so no key acts behind it. */
export function Sheet({ title, closeLabel, description, actions, leadingActions, busy, onClose, children }: {
  title: string; closeLabel: string;
  /** One quiet sentence under the title that says what the sheet is for; it describes the dialog. */
  description?: ReactNode;
  /** Buttons on the trailing edge of the bottom row; the primary goes last. */
  actions?: ReactNode;
  /** Buttons on the leading edge of the bottom row. */
  leadingActions?: ReactNode;
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
  return <div className="ui-sheet-backdrop" onPointerDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <div ref={dialog} role="dialog" aria-modal="true" aria-labelledby={titleId} aria-describedby={description ? aboutId : undefined}
      // Clicking plain text inside keeps focus here, so Escape still closes it.
      tabIndex={-1} className="ui-overlay ui-sheet"
      onKeyDown={(event) => {
        if (event.key !== "Tab") return;
        const focusable = [...(dialog.current?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? [])];
        const at = focusable.indexOf(document.activeElement as HTMLElement);
        event.preventDefault();
        focusable[at < 0 ? event.shiftKey ? focusable.length - 1 : 0 : (at + (event.shiftKey ? -1 : 1) + focusable.length) % focusable.length]?.focus();
      }}>
      <div className="ui-sheet-heading">
        <h2 id={titleId}>{title}</h2>
        <IconButton aria-label={closeLabel} title={closeLabel} aria-keyshortcuts="Escape" onPress={onClose} />
      </div>
      <div ref={body} className="ui-sheet-body scroll-area" tabIndex={0} role="group" aria-labelledby={titleId} aria-busy={busy || undefined}>
        {description && <p id={aboutId} className="ui-sheet-note">{description}</p>}
        {children}
      </div>
      {(actions || leadingActions) && <div className="ui-sheet-actions">
        {leadingActions}
        <span className="ui-sheet-spacer" />
        {actions}
      </div>}
    </div>
  </div>;
}

/** A list of things in a sheet, one per row. */
export function SheetList({ label, children }: { label: string; children: ReactNode }) {
  return <ul className="ui-sheet-list" aria-label={label}>{children}</ul>;
}

/** One row of a sheet's list: what it is first, then its buttons. */
export function SheetRow({ children }: { children: ReactNode }) {
  return <li className="ui-sheet-row">{children}</li>;
}

/** A part of a sheet's body under a divider, named by its heading. */
export function SheetSection({ label, heading, children }: { label: string; heading?: ReactNode; children: ReactNode }) {
  return <section className="ui-sheet-section" aria-label={label}>{heading}{children}</section>;
}

/** Buttons that act on what is just above them, inside the body. */
export function SheetButtons({ children }: { children: ReactNode }) {
  return <div className="ui-sheet-buttons">{children}</div>;
}
