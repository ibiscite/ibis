import type React from "react";
import type { LibraryStatus } from "../../api";
import { ariaKeys, keyLabel, keysFor } from "../../keys";
import { LibraryStatusLine } from "../../LibraryStatusLine";
import { keyText, pickerText } from "../../messages";
import { isMac } from "../../platform";

const label = (accelerator: string) => keyLabel(accelerator, isMac);

/** What the keys do now; null where a key does nothing. */
export type KeyHints = { enter: string | null; shiftEnter: string | null; escape: string };

/** The footer: library status, what Enter (or, with Alt held, the other keys)
 * does now, and the More button with its menu. */
export function PickerFooter({ library, groupName, hints, altHeld, searchFocused, moreButtonRef, menuOpen, onMoreClick, menu }: {
  library: LibraryStatus | null | undefined;
  groupName: (id: number) => string;
  hints: KeyHints;
  /** While Alt (⌥) is held, the footer shows the other keys instead of Enter. */
  altHeld: boolean;
  /** Search-box keys are hidden from screen readers while focus is elsewhere. */
  searchFocused: boolean;
  moreButtonRef: React.Ref<HTMLButtonElement>;
  menuOpen: boolean;
  onMoreClick: () => void;
  /** The More menu, while it is open. */
  menu: React.ReactNode;
}) {
  return (
    <footer>
      <section aria-label={pickerText.libraryRegion} className="library-bar">
        <LibraryStatusLine library={library} groupName={groupName} />
        <small className="enter-hints" aria-label={keyText.footer} role="group">
          {altHeld ? <>
            {hints.shiftEnter && <span className="search-hint" aria-hidden={!searchFocused}><kbd>{label(keysFor("add"))}</kbd> {hints.shiftEnter}</span>}
            <span><kbd>{label(keysFor("escape"))}</kbd> {hints.escape}</span>
          </> : <>
            {hints.enter && <span className="search-hint" aria-hidden={!searchFocused}><kbd>{label(keysFor("copy"))}</kbd> {hints.enter}</span>}
            <span><kbd>{label(keysFor("keys"))}</kbd> {keyText.allKeys}</span>
          </>}
        </small>
        <span className="more">
          <button
            type="button"
            ref={moreButtonRef}
            className="quiet-button"
            aria-haspopup="menu"
            aria-keyshortcuts={ariaKeys(keysFor("more"), isMac)}
            aria-expanded={menuOpen}
            aria-controls={menuOpen ? "more-menu" : undefined}
            title={pickerText.moreTitle(label(keysFor("more")))}
            onClick={onMoreClick}
          >
            {pickerText.more} <kbd aria-hidden="true">{label(keysFor("more"))}</kbd>
          </button>
          {menu}
        </span>
        {library?.note && <p role="status">{library.note}</p>}
      </section>
    </footer>
  );
}
