import type { ReactNode } from "react";
import "./SettingsGroup.css";

/** A grouped Settings card. Groups keep their natural height in a page scroller. */
export function SettingsGroup({ id, title, children }: { id: string; title?: string; children: ReactNode }) {
  return (
    <section className="ui-settings-group" id={id} aria-labelledby={title ? `${id}-title` : undefined}>
      {title && <h2 id={`${id}-title`} className="ui-settings-group-title">{title}</h2>}
      <div className="ui-settings-card">{children}</div>
    </section>
  );
}

/** An inset label and description, with a trailing control that can wrap below. */
export function SettingsRow({ id, label, description, children, problem, highlighted }: { id: string; label: string; description?: string; children?: ReactNode; problem?: string; highlighted?: boolean }) {
  return (
    <div className="ui-settings-row" data-described={description ? true : undefined} data-highlighted={highlighted || undefined}>
      <div className="ui-settings-row-text">
        <span id={id}>{label}</span>
        {description && <span id={`${id}-description`} className="ui-settings-description">{description}</span>}
      </div>
      {children}
      {problem && <p className="ui-settings-row-problem" role="alert">{problem}</p>}
    </div>
  );
}
