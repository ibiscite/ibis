import { Switch as AriaSwitch, type SwitchProps } from "react-aria-components";
import "./Switch.css";
import type { AccessibleName } from "./AccessibleName";

/** The one on/off control (ui/README.md, Switch; sizes are the --switch-* tokens).
 * Its accessible name comes from `aria-label` or `aria-labelledby`. */
export function Switch({ className = "", ...props }: Omit<SwitchProps, "className" | "children"> & { className?: string } & AccessibleName) {
  return <AriaSwitch {...props} className={`ui-switch ${className}`}><span className="ui-switch-track" aria-hidden="true"><span className="ui-switch-thumb" /></span></AriaSwitch>;
}
