import { useState } from "react";
import { composing } from "../composition";
import { shortcutLabel } from "../keys";
import { currentPlatform } from "../platform";
import { Button } from "./Button";

/** Capture before button activation or browser commands consume the key. */
export function ShortcutRecorder({ shortcut, label, prompt, isDisabled, onRecord }: {
  shortcut: string; label: string; prompt: string; isDisabled: boolean; onRecord: (shortcut: string) => void;
}) {
  const [recording, setRecording] = useState(false);
  return (
    <span data-recording={recording || undefined} onKeyDownCapture={(event) => {
      if (!recording || isDisabled) return;
      if (event.key === "Tab" && !event.ctrlKey && !event.altKey && !event.metaKey) {
        setRecording(false);
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") { setRecording(false); return; }
      if (event.repeat || composing(event) || ["Control", "Alt", "Shift", "Meta", "AltGraph"].includes(event.key)) return;
      const key = /^[a-z]$/i.test(event.key) ? event.key.toUpperCase() : event.code;
      if (!key) return;
      const keys = [event.ctrlKey && "Control", event.altKey && "Alt", event.shiftKey && "Shift", event.metaKey && "Command", key];
      setRecording(false);
      onRecord(keys.filter(Boolean).join("+"));
    }}>
      <Button aria-label={`${label}: ${recording ? prompt : shortcutLabel(shortcut, currentPlatform() === "macos")}`}
        aria-pressed={recording} isDisabled={isDisabled} onBlur={() => setRecording(false)} onPress={() => setRecording(true)}>
        {recording ? prompt : shortcutLabel(shortcut, currentPlatform() === "macos")}
      </Button>
    </span>
  );
}
