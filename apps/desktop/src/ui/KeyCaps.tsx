import { keyCapLabels } from "../keys";
import { currentPlatform, type Platform } from "../platform";
import "./KeyCaps.css";
import { useKeyPulse } from "./Delights";
import { createContext, useContext } from "react";
import { usePresenceSlot } from "./presence";

export const ButtonKeys = createContext<((slot: "keys", on: boolean) => void) | undefined>(undefined);

/** One cap per shortcut on every system (docs/design/picker.md, Footer), written as
 * that system's menus write it: ⌘K on macOS, Ctrl+K on Windows and Linux. */
export function KeyCaps({ accelerator, platform = currentPlatform(), hidden, echo = false }: {
  accelerator: string; platform?: Platform;
  /** Hidden from screen readers where the control already names its keys. */
  hidden?: boolean;
  echo?: boolean;
}) {
  usePresenceSlot(useContext(ButtonKeys), "keys");
  const pressed = useKeyPulse(accelerator, echo);
  const parts = keyCapLabels(accelerator, platform === "macos");
  return <span className="key-caps" data-echo={pressed || undefined} aria-hidden={hidden || undefined}>
    {parts.map((part, i) => <span key={i} className="key-cap">
<kbd data-single={[...part].length === 1 || undefined} data-label={part}>{part}</kbd>
    </span>)}
  </span>;
}

/** A label and its keys, always in that order: the one hint pattern of the
 * footer and the empty states. The primary hint is in the text colour. */
export function KeyHint({ label, keys, primary, platform, hidden, echo }: {
  label: string; keys: string; primary?: boolean; platform?: Platform; hidden?: boolean; echo?: boolean;
}) {
  return <span className="key-hint" data-primary={primary || undefined}>
    <span className="key-hint-label">{label}</span>{" "}<KeyCaps accelerator={keys} platform={platform} hidden={hidden} echo={echo} />
  </span>;
}
