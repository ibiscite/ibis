import type { ReactNode } from "react";
import { ProgressBar } from "react-aria-components";

/** Accessible indeterminate progress; the caller supplies its visual content. */
export function IndeterminateProgress({ label, className, children }: {
  label: string;
  className: string;
  children: ReactNode;
}) {
  return <ProgressBar isIndeterminate aria-label={label} className={className}>{children}</ProgressBar>;
}
