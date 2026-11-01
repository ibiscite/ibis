import "./SettingsGroup.css";

/** The same labelled search field in the sidebar and shortcut catalogue. */
export function SettingsSearch({ label, value, onChange, className = "" }: {
  label: string; value: string; onChange: (value: string) => void; className?: string;
}) {
  return <div className={`ui-settings-search ${className}`}>
    <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor"><path d="M10.5 4a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM16 16l5 5" /></svg>
    <input type="search" aria-label={label} placeholder={label} value={value} onChange={(event) => onChange(event.target.value)} />
  </div>;
}
