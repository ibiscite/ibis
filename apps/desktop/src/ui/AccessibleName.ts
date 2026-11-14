/** A control cannot be constructed without a name. Visible string children
 * are a name too; icons and composed content need an explicit label. */
export type AccessibleName =
  | { "aria-label": string }
  | { "aria-labelledby": string };
export type ButtonName = AccessibleName | { label: string } | { children: string };
