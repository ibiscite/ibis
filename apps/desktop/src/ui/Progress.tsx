import type { ReactNode } from "react";
import "./Progress.css";
import { ProgressBar } from "react-aria-components";

/** Accessible indeterminate progress; the caller supplies its visual content. */
export function IndeterminateProgress({ label, className, children }: {
  label: string;
  className: string;
  children: ReactNode;
}) {
  return <ProgressBar isIndeterminate aria-label={label} className={className}>{children}</ProgressBar>;
}

/** Known percentages and honest indeterminate phases share the same wrapper. */
export function Progress({ label, value }: { label: string; value?: number }) {
  const indeterminate = value === undefined;
  return <ProgressBar aria-label={label} value={value} isIndeterminate={indeterminate} data-indeterminate={indeterminate || undefined} className="update-progress">
    <span>{label}{!indeterminate && ` ${Math.floor(value)}%`}</span>
    <div className="update-track" aria-hidden="true"><div style={indeterminate ? undefined : { width: `${value}%` }} /></div>
  </ProgressBar>;
}
