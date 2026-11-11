import { useEffect, useId, useLayoutEffect, useRef, type ReactNode } from "react";
import { Button as AriaButton } from "react-aria-components";
import { Icon } from "../icons";
import { useLayer } from "./layers";
import { Button } from "./Button";
import { ViewFrame } from "./ViewFrame";
import { useScrollEdges } from "./ScrollArea";
import "./SettingsGroup.css";
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
        {description && <p id={aboutId} className="ui-sheet-note">{description}</p>}
        {children}
      </div>
    </div>
  </ViewFrame>;
}

/** A list of things in a sheet, one per row. */
export function SheetList({ label, children }: { label: string; children: ReactNode }) {
  return <ul className="ui-settings-card ui-sheet-list" aria-label={label}>{children}</ul>;
}

/** One row of a sheet's list, drawn as a Settings row on a grouped card: what
 * it is first, then its buttons. */
export function SheetRow({ children }: { children: ReactNode }) {
  return <li className="ui-settings-row ui-sheet-row">{children}</li>;
}

/** One choice in a sheet's list, as System Settings lists choices: the whole row
 * is the button, its name and a quiet detail at the start, a check at the end
 * on the chosen one. */
export function SheetChoice({ label, detail, accessibleLabel, selected, disabled, onPress }: {
  label: string; detail?: ReactNode; accessibleLabel?: string; selected: boolean; disabled?: boolean; onPress: () => void;
}) {
  return <li className="ui-settings-row ui-sheet-row ui-sheet-choice-row">
    <AriaButton className="ui-sheet-choice" aria-label={accessibleLabel} aria-pressed={selected} isDisabled={disabled} onPress={onPress}>
      <span className="ui-settings-row-text"><span dir="auto">{label}</span>{detail && <span className="ui-settings-description" dir="auto">{detail}</span>}</span>
      <span className="ui-sheet-check" aria-hidden="true">{selected && <Icon name="check" />}</span>
    </AriaButton>
  </li>;
}

/** A part of a sheet's body named by its heading, as a Settings group is. */
export function SheetSection({ label, heading, children }: { label: string; heading?: ReactNode; children: ReactNode }) {
  return <section className="ui-sheet-section" aria-label={label}>{heading}{children}</section>;
}

/** Buttons that act on what is just above them, inside the body. */
export function SheetButtons({ children }: { children: ReactNode }) {
  return <div className="ui-sheet-buttons">{children}</div>;
}
