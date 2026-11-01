import { IconButton } from "./IconButton";
import { useEffect, useRef } from "react";
import { useInterfaceMessages } from "../i18n";
import { type KeyBinding } from "../keys";
import { ShortcutGroup } from "./ShortcutTable";
import { useScrollEdges } from "./ScrollArea";
import "./Overlay.css";
import "./KeysSheet.css";

/** The picker's keys in two compact columns of groups, as a modal dialog:
    Escape or Close returns focus. */
export function KeysDialog({ bindings, isMac, onClose }: { bindings: readonly KeyBinding[]; isMac: boolean; onClose: () => void }) {
  const { text } = useInterfaceMessages();
  const dialog = useRef<HTMLDivElement>(null);
  const scroll = useRef<HTMLDivElement>(null);
  useScrollEdges(scroll);
  useEffect(() => { dialog.current?.focus(); }, []);
  return (
    <div className="keys-backdrop" onPointerDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        // Clicking plain text inside keeps focus here, so Escape still closes it.
        tabIndex={-1}
        aria-labelledby="keys-title"
        className="keys-dialog ui-overlay ui-sheet"
        onKeyDown={(e) => {
          // The picker's own keys wait until the dialog is closed.
          e.stopPropagation();
          if (e.key === "Escape") {
            e.preventDefault();
            onClose();
          } else if (e.key === "Tab") {
            const focusable = [...(dialog.current?.querySelectorAll<HTMLElement>("button, [tabindex='0']") ?? [])];
            const at = focusable.indexOf(document.activeElement as HTMLElement);
            e.preventDefault();
            focusable[at < 0 ? e.shiftKey ? focusable.length - 1 : 0 : (at + (e.shiftKey ? -1 : 1) + focusable.length) % focusable.length]?.focus();
          }
        }}
      >
        <div className="keys-heading">
          <h2 id="keys-title">{text("keys-title")}</h2>
          <IconButton aria-label={text("keys-close")} title={text("keys-close")} aria-keyshortcuts="Escape" onPress={onClose} />
        </div>
        <div ref={scroll} className="keys-scroll scroll-area" tabIndex={0} role="group" aria-labelledby="keys-title">
          <div className="keys-groups">
            {([ ["search", "library", "system"], ["picker"] ] as const).map((regions, column) => <table key={column}>
              <thead className="visually-hidden"><tr><th scope="col">{text("keys-action")}</th><th scope="col">{text("keys-shortcut")}</th></tr></thead>
              {regions.filter((region) => bindings.some((binding) => binding.region === region)).map((region) => <ShortcutGroup key={region} region={region} bindings={bindings} isMac={isMac} />)}
            </table>)}
          </div>
        </div>
      </div>
    </div>
  );
}
