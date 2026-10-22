import type React from "react";
import type { LibraryStatus } from "../../api";
import { ariaKeys, keysFor } from "../../keys";
import { LibraryStatusLine } from "../../LibraryStatusLine";
import { keyText, pickerText } from "../../messages";
import { currentPlatform } from "../../platform";
import { KeyHint } from "../../ui/KeyCaps";

/** What the keys do now; null where a key does nothing. */
export type KeyHints = { enter: string | null; shiftEnter: string | null; escape: string };

/** The footer: library status, what Enter (or, with Alt held, the other keys)
 * does now, and the More button with its menu. */
export function PickerFooter({ library, groupName, hints, altHeld, actions, more }: {
  library: LibraryStatus | null | undefined;
  groupName: (id: number) => string;
  hints: KeyHints;
  /** While Alt (⌥) is held, the footer shows the other keys instead of Enter. */
  altHeld: boolean;
  actions: { copy: () => void; add: () => void; escape: () => void; keys: () => void };
  /** The More button and its menu. */
  more: React.ReactNode;
}) {
  return (
    <footer>
      <section aria-label={pickerText.libraryRegion} className="library-bar">
        <LibraryStatusLine library={library} groupName={groupName} />
        <small className="enter-hints" aria-label={keyText.footer} role="group">
          {altHeld ? <>
            {hints.shiftEnter && <FooterAction label={hints.shiftEnter} keys={keysFor("add")} onAction={actions.add} />}
            <FooterAction label={hints.escape} keys={keysFor("escape")} onAction={actions.escape} />
          </> : <>
            {hints.enter && <FooterAction label={hints.enter} keys={keysFor("copy")} primary onAction={actions.copy} />}
            <FooterAction label={keyText.allKeys} keys={keysFor("keys")} className="shortcuts-hint" onAction={actions.keys} />
          </>}
        </small>
        <span className="more">
          {more}
        </span>
        {library?.note && <p role="status">{library.note}</p>}
      </section>
    </footer>
  );
}

function FooterAction({ label, keys, primary, className = "", onAction }: {
  label: string; keys: string; primary?: boolean; className?: string; onAction: () => void;
}) {
  const platform = currentPlatform();
  return <button type="button" className={`quiet-button footer-action ${className}`} aria-label={label}
    aria-keyshortcuts={ariaKeys(keys, platform === "macos")} onClick={onAction}>
    <KeyHint label={label} keys={keys} primary={primary} platform={platform} hidden />
  </button>;
}
