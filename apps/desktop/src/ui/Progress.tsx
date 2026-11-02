import type { CSSProperties, ReactNode } from "react";
import "./Progress.css";
import { ProgressBar } from "react-aria-components";
import { IbisMark } from "./IbisMark";

/** Accessible indeterminate progress; the caller supplies its visual content. */
export function IndeterminateProgress({ label, className, children }: {
  label: string;
  className: string;
  children: ReactNode;
}) {
  return <ProgressBar isIndeterminate data-indeterminate aria-label={label} className={className}>{children}</ProgressBar>;
}

/** One task in progress as a quiet card on the row grid: a ring around the
 * Ibis mark in the tile column, the title and phase in the text column, the percent at the end.
 * A known percentage fills the ring; an honest unknown phase turns it. */
export function Progress({ title, label, value, detail }: { title?: string; label: string; value?: number; detail?: string }) {
  const indeterminate = value === undefined;
  return <ProgressBar aria-label={title ? `${title}, ${label}` : label} value={value} isIndeterminate={indeterminate} data-indeterminate={indeterminate || undefined} className="update-card">
    <span className="update-ring" aria-hidden="true">
      <svg viewBox="0 0 20 20">
        <circle className="update-ring-fill" cx="10" cy="10" r="8.75" pathLength={100}
          style={indeterminate ? undefined : { "--progress": value } as CSSProperties} />
      </svg>
      <IbisMark className="update-ring-mark" size="search" />
    </span>
    <span className="update-text">
      <strong>{title ?? label}</strong>
      {(title || detail) && <span>{[title ? label : null, detail].filter(Boolean).join(" · ")}</span>}
    </span>
    {!indeterminate && <span className="update-percent">{Math.floor(value)}%</span>}
  </ProgressBar>;
}
