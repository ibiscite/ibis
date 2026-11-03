import type { CSSProperties, ReactNode } from "react";
import "./Progress.css";
import { ProgressBar } from "react-aria-components";

/** Accessible indeterminate progress; the caller supplies its visual content. */
export function IndeterminateProgress({ label, className, children }: {
  label: string;
  className: string;
  children: ReactNode;
}) {
  return <ProgressBar isIndeterminate data-indeterminate aria-label={label} className={className}>{children}</ProgressBar>;
}

/** One task in progress as one line of the notice slot: a small ring where the status
 * dot sits, then the title, phase and percent. A known percentage fills the ring; an
 * honest unknown phase turns it. */
export function Progress({ title, label, value, detail }: { title?: string; label: string; value?: number; detail?: string }) {
  const indeterminate = value === undefined;
  return <ProgressBar aria-label={title ? `${title}, ${label}` : label} value={value} isIndeterminate={indeterminate} data-indeterminate={indeterminate || undefined} className="update-card">
    <span className="update-ring" aria-hidden="true">
      <svg viewBox="0 0 20 20">
        <circle className="update-ring-fill" cx="10" cy="10" r="8.75" pathLength={100}
          style={indeterminate ? undefined : { "--progress": value } as CSSProperties} />
      </svg>
    </span>
    <span className="update-text">
      <strong>{title ?? label}</strong>
      {(title || detail) && <span>{[title ? label : null, detail].filter(Boolean).join(" · ")}</span>}
    </span>
    {!indeterminate && <span className="update-percent">{Math.floor(value)}%</span>}
  </ProgressBar>;
}
