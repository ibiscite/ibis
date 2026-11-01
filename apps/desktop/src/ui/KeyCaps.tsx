import { keyCapLabels } from "../keys";
import { currentPlatform, type Platform } from "../platform";
import "./KeyCaps.css";
import { useKeyPulse } from "./Delights";

/** The keys of an accelerator as each system writes them (docs/design.md,
 * Footer): one cap of joined glyphs on macOS (⌘K, as its menus write it), one
 * cap per key on Windows (Ctrl K, as PowerToys does), and on GNOME one cap per
 * key with a "+" between (Ctrl + K, as GTK's shortcut labels do). */
export function KeyCaps({ accelerator, platform = currentPlatform(), hidden, echo = false }: {
  accelerator: string; platform?: Platform;
  /** Hidden from screen readers where the control already names its keys. */
  hidden?: boolean;
  echo?: boolean;
}) {
  const pressed = useKeyPulse(accelerator, echo);
  const parts = keyCapLabels(accelerator, platform === "macos");
  return <span className="key-caps" data-echo={pressed || undefined} aria-hidden={hidden || undefined}>
    {parts.map((part, i) => <span key={i} className="key-cap">
      {i > 0 && platform === "linux" && <span className="key-plus">+</span>}<kbd>{part}</kbd>
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
