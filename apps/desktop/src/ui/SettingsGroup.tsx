import type { ReactNode } from "react";

/** Shared Settings rows: text first, then a control that wraps when needed. */
export function SettingsRow({ id, label, description, children }: { id: string; label: string; description?: string; children?: ReactNode }) {
  return (
    <div className="settings-row" data-described={description ? true : undefined}>
      <div className="settings-row-text">
        <span id={id} className="settings-label">{label}</span>
        {description && <span id={`${id}-description`} className="settings-description">{description}</span>}
      </div>
      {children}
    </div>
  );
}

export function SettingsGroup({ id, title, children }: { id?: string; title?: string; children: ReactNode }) {
  return (
    <section className="settings-group" id={id} aria-labelledby={title ? `${id}-title` : undefined}>
      {title && <h2 id={`${id}-title`} className="settings-group-title">{title}</h2>}
      <div className="settings-card">{children}</div>
    </section>
  );
}
