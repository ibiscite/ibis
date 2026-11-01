import type { KeyboardEvent as ReactKeyboardEvent } from "react";

/** Enter confirms composing text rather than a command. Some WebKit versions
 * expose only key code 229; controls may also retain composition-start state. */
export function composing(event: ReactKeyboardEvent | KeyboardEvent, active = false): boolean {
  return active || ("nativeEvent" in event ? event.nativeEvent.isComposing : event.isComposing) || event.keyCode === 229;
}
