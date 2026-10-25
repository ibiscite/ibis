import type { KeyboardEvent } from "react";

/** Enter confirms composing text rather than a command. Some WebKit versions
 * expose only key code 229; controls may also retain composition-start state. */
export function composing(event: KeyboardEvent, active = false): boolean {
  return active || event.nativeEvent.isComposing || event.keyCode === 229;
}
