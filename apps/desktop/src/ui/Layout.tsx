import type { ReactNode } from "react";
import { Button as AriaButton } from "react-aria-components";
import { Icon } from "../icons";
import "./Layout.css";

/** The layout parts (maintainer, 2026-11-10). Screens compose these and never set
 * their own margin, padding, gap or position, so left edges line up by
 * construction: a card's edge on the window inset, a heading, a description and a
 * row's words on the heading offset, and an icon row's words on the label column
 * after the icon slot. */

/** A part of a page or sheet named by its heading: the heading and an optional
 * description on the row text's edge, then its content (usually a Group). */
export function Section({ id, title, label, level = 2, description, children }: {
  id?: string;
  /** The visible heading. */
  title?: ReactNode;
  /** The spoken name, when there is no visible heading or it says less. */
  label?: string;
  level?: 2 | 3;
  description?: ReactNode;
  children: ReactNode;
}) {
  const Heading = level === 2 ? "h2" : "h3";
  const titleId = id && title ? `${id}-title` : undefined;
  return <section className="ui-section" id={id} aria-label={label} aria-labelledby={label ? undefined : titleId}>
    {title && <Heading id={titleId} className="ui-section-title" dir="auto">{title}</Heading>}
    {description && <p className="ui-section-description">{description}</p>}
    {children}
  </section>;
}

/** The card: grouped rows on the grouped fill, edge to edge in its section. A
 * labelled group is a list, so its rows are list items. */
export function Group({ label, children }: { label?: string; children: ReactNode }) {
  return label ? <ul className="ui-group" aria-label={label}>{children}</ul> : <div className="ui-group">{children}</div>;
}

/** One row of a Group: an optional icon in the icon slot, the label with a quiet
 * description, then its accessory (a control or buttons) at the end, which wraps
 * below when the row is narrow. `labelHidden` keeps the label for screen readers
 * only, for a row whose control names itself (a picture choice). */
export function Row({ id, label, description, icon, children, problem, highlighted, labelHidden = false, item = false }: {
  id?: string; label: ReactNode; description?: ReactNode; icon?: ReactNode; children?: ReactNode;
  problem?: string; highlighted?: boolean; labelHidden?: boolean;
  /** In a labelled Group (a list). */
  item?: boolean;
}) {
  const Element = item ? "li" : "div";
  return <Element className="ui-row" data-bare={labelHidden || undefined} data-highlighted={highlighted || undefined}>
    <span className={labelHidden ? "visually-hidden" : "ui-row-lead"}>
      {icon && !labelHidden && <span className="ui-row-icon" aria-hidden="true">{icon}</span>}
      <span className="ui-row-text">
        <span id={id} dir="auto">{label}</span>
        {description && <span id={id ? `${id}-description` : undefined} className="ui-row-description" dir="auto">{description}</span>}
      </span>
    </span>
    {children}
    {problem && <p className="ui-row-problem" role="alert">{problem}</p>}
  </Element>;
}

/** A row to choose from a list, as System Settings lists choices: the whole row is
 * the button, its label and description at the start, a check at the end on the
 * chosen one. */
export function ChoiceRow({ label, description, accessibleLabel, selected, disabled, onPress }: {
  label: string; description?: ReactNode; accessibleLabel?: string; selected: boolean; disabled?: boolean; onPress: () => void;
}) {
  return <li className="ui-row ui-choice-row">
    <AriaButton className="ui-choice" aria-label={accessibleLabel} aria-pressed={selected} isDisabled={disabled} onPress={onPress}>
      <span className="ui-row-text"><span dir="auto">{label}</span>{description && <span className="ui-row-description" dir="auto">{description}</span>}</span>
      <span className="ui-choice-check" aria-hidden="true">{selected && <Icon name="check" />}</span>
    </AriaButton>
  </li>;
}

/** Buttons that act on what is just above them, in one wrapping line. */
export function Buttons({ children, end = false }: { children: ReactNode; end?: boolean }) {
  return <div className="ui-buttons" data-end={end || undefined}>{children}</div>;
}

/** Quiet words under a row or a section: a note, a status, a problem. */
export function Note({ children, role, problem = false, list = false }: { children: ReactNode; role?: "status" | "alert"; problem?: boolean; list?: boolean }) {
  return list ? <ul className="ui-note">{children}</ul> : <p className="ui-note" role={role} data-problem={problem || undefined}>{children}</p>;
}
