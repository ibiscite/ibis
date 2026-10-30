import type { SelectHTMLAttributes } from "react";
import "./SettingsGroup.css";

/** Native select for named, nonvisual settings such as citation syntax. */
export function Select(props: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select {...props} className="ui-settings-select" />;
}
