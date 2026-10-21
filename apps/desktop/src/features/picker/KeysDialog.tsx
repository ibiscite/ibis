import { useEffect, useRef } from "react";
import { keyText } from "../../messages";
import { ariaKeys, keyLabel, shortcutLabel, type KeyBinding } from "../../keys";

/** Every key in one table, as a modal dialog: Escape or Close returns focus. */
export function KeysDialog({ bindings, isMac, onClose }: { bindings: readonly KeyBinding[]; isMac: boolean; onClose: () => void }) {
  const dialog = useRef<HTMLDivElement>(null);
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
        className="keys-dialog"
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
          <button type="button" className="quiet-button" onClick={onClose}>{keyText.close}</button>
        </div>
        <div className="keys-scroll" tabIndex={0} role="group" aria-labelledby="keys-title">
          <table>
            <thead>
              <tr><th scope="col">{keyText.keyColumn}</th><th scope="col">{keyText.actionColumn}</th><th scope="col">{keyText.regionColumn}</th></tr>
            </thead>
            <tbody>
              {bindings.map((binding) => (
                <tr key={`${binding.region}:${binding.action}`}>
                  <td>{binding.keys.map((keys, i) => (
                    <span key={keys}>{i > 0 && ` ${keyText.or} `}<kbd aria-keyshortcuts={ariaKeys(keys, isMac)}>{binding.action === "open" ? shortcutLabel(keys, isMac) : keyLabel(keys, isMac)}</kbd></span>
                  ))}</td>
                  <td>{keyText.actions[binding.action]}</td>
                  <td>{keyText.regions[binding.region]}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
