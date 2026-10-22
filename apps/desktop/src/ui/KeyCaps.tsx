import { keyLabel } from "../keys";
import { currentPlatform, type Platform } from "../platform";
import "./KeyCaps.css";

/** The keys of an accelerator as each system writes them (docs/design.md,
 * Footer): one cap of joined glyphs on macOS (⌘K, as its menus write it), one
 * cap per key on Windows (Ctrl K, as PowerToys does), and on GNOME one cap per
 * key with a "+" between (Ctrl + K, as GTK's shortcut labels do). */
export function KeyCaps({ accelerator, platform = currentPlatform(), hidden }: {
  accelerator: string; platform?: Platform;
  /** Hidden from screen readers where the control already names its keys. */
  hidden?: boolean;
}) {
  const mac = platform === "macos";
  const label = keyLabel(accelerator, mac);
  // keyLabel joins with "+" off macOS; a lone or trailing "+" is the key itself.
  const parts = mac ? [label] : label === "+" ? ["+"] : label.split(/\+(?!$)/);
  return <span className="key-caps" aria-hidden={hidden || undefined}>
    {parts.map((part, i) => <span key={i} className="key-cap">
      {i > 0 && platform === "linux" && <span className="key-plus">+</span>}<kbd>{part}</kbd>
    </span>)}
  </span>;
}

/** A label and its keys, always in that order: the one hint pattern of the
 * footer and the empty states. The primary hint is in the text colour. */
export function KeyHint({ label, keys, primary, platform, hidden }: {
  label: string; keys: string; primary?: boolean; platform?: Platform; hidden?: boolean;
}) {
  return <span className="key-hint" data-primary={primary || undefined}>
    <span className="key-hint-label">{label}</span>{" "}<KeyCaps accelerator={keys} platform={platform} hidden={hidden} />
  </span>;
}
