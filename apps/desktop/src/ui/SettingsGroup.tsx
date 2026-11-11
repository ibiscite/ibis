import type { ReactNode } from "react";
import { useInterfaceMessages } from "../i18n";
import { Button } from "./Button";
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
export function SettingsRow({ id, label, description, children, problem, highlighted, bare = false, changed = false, onReset }: { id: string; label: ReactNode; description?: string; children?: ReactNode; problem?: string; bare?: boolean; highlighted?: boolean; changed?: boolean; onReset?: () => void }) {
  const { text } = useInterfaceMessages();
  return (
    <div className="ui-settings-row" data-bare={bare || undefined} data-described={description ? true : undefined} data-highlighted={highlighted || undefined}>
      <div className={bare ? "visually-hidden" : "ui-settings-row-text"}>
        <span id={id}>{label}</span>
        {changed && <span className="ui-settings-description">{text("settings-changed")}</span>}
        {description && <span id={`${id}-description`} className="ui-settings-description">{description}</span>}
      </div>
      {children}
      {changed && onReset && <Button variant="quiet" size="small" aria-label={text("settings-reset-row", { setting: typeof label === "string" ? label : "" })} onPress={onReset}>{text("settings-reset")}</Button>}
      {problem && <p className="ui-settings-row-problem" role="alert">{problem}</p>}
    </div>
  );
}
