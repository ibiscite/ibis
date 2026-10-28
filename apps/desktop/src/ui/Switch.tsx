import { Switch as AriaSwitch, type SwitchProps } from "react-aria-components";
import "./Switch.css";

/** The one on/off control (docs/design.md, Switch: 38 × 22 on the shared look).
 * Its accessible name comes from `aria-label` or `aria-labelledby`. */
export function Switch({ className = "", ...props }: Omit<SwitchProps, "className" | "children"> & { className?: string }) {
  return <AriaSwitch {...props} className={`ui-switch ${className}`}><span className="ui-switch-track" aria-hidden="true"><span className="ui-switch-thumb" /></span></AriaSwitch>;
}
