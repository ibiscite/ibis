import { useEffect, useRef } from "react";
import { keyText } from "../messages";
import { ariaKeys, type KeyBinding } from "../keys";
import { Button } from "./Button";
import { KeyCaps } from "./KeyCaps";
import { useScrollEdges } from "./ScrollArea";
import "./Overlay.css";
import "./KeysSheet.css";

/** The picker's keys in two compact columns of groups, as a modal dialog:
    Escape or Close returns focus. */
export function KeysDialog({ bindings, isMac, onClose }: { bindings: readonly KeyBinding[]; isMac: boolean; onClose: () => void }) {
  const regions = [...new Set(bindings.filter((binding) => binding.region !== "menu").map((binding) => binding.region))];
  const dialog = useRef<HTMLDivElement>(null);
  const scroll = useRef<HTMLDivElement>(null);
  useScrollEdges(scroll);
  useEffect(() => { dialog.current?.querySelector<HTMLElement>("button")?.focus(); }, []);
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
            focusable[(at + (e.shiftKey ? -1 : 1) + focusable.length) % focusable.length]?.focus();
          }
        }}
      >
        <div className="keys-heading">
          <h2 id="keys-title">{keyText.title}</h2>
          <Button variant="icon" glyph="close" aria-label={keyText.close} title={keyText.close} aria-keyshortcuts="Escape" onPress={onClose} />
        </div>
        <div ref={scroll} className="keys-scroll scroll-area" tabIndex={0} role="group" aria-labelledby="keys-title">
          <table className="keys-groups">
            <thead className="visually-hidden">
              <tr><th scope="col">{keyText.actionColumn}</th><th scope="col">{keyText.keyColumn}</th></tr>
            </thead>
            {regions.map((region) => (
              <tbody key={region}>
                <tr><th scope="rowgroup" colSpan={2}>{keyText.regions[region]}</th></tr>
                {bindings.filter((binding) => binding.region === region).map((binding) => (
                  <tr key={binding.action}>
                    <th scope="row">{keyText.actions[binding.action]}</th>
                    <td>{binding.keys.map((keys, i) => (
                      <span className="keys-binding" key={keys} aria-keyshortcuts={binding.displayLabel ? undefined : ariaKeys(keys, isMac)}>
                        {i > 0 && <>{" "}<span>{keyText.or}</span>{" "}</>}
                        {binding.displayLabel ? <kbd dir="ltr">{binding.displayLabel}</kbd> : <KeyCaps accelerator={keys} platform={isMac ? "macos" : undefined} />}
                      </span>
                    ))}</td>
                  </tr>
                ))}
              </tbody>
            ))}
          </table>
        </div>
      </div>
    </div>
  );
}
