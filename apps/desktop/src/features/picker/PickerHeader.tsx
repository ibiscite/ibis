import type React from "react";
import { mark as ibisMark } from "../../brand";
import { Button, ToggleButton } from "../../ui/Button";
import { composing } from "./shortcuts";
import { pickerText } from "../../messages";
import { ariaKeys, keysFor } from "../../keys";
import { currentPlatform } from "../../platform";
import type { Choice } from "./choice";
import { FormatMenu, type FormatCommand } from "./FormatMenu";
import type { MenuCloseReason, ToolbarMenuCommand } from "../../ui/ToolbarMenu";

/** The search bar: the search box and the copy format. It is also
 * the window's drag region; the editable box itself is not. */
export function PickerHeader({ inputRef, query, onQueryChange, onSearchKeyDown, expanded, activeDescendant, format, formatButtonRef, app = null, formatScope, onFormatOpenChange, onceOnly = false, onToggleOnce = () => undefined, onAutomatic = () => undefined, onDefault = () => undefined, formatDisabled = false, showFormat, styles, onFormatChange, onFormatCommand, pinned, onTogglePin, mark }: {
  inputRef: React.RefObject<HTMLInputElement | null>;
  pinned: boolean;
  /** The Ibis mark in place of the magnifier, a person's choice in More. */
  mark?: boolean;
  onTogglePin: () => void;
  query: string;
  onQueryChange: (query: string) => void;
  onSearchKeyDown: (e: React.KeyboardEvent<HTMLInputElement>) => void;
  /** Whether the result list has rows. */
  expanded: boolean;
  /** The id of the highlighted row, when there is one. */
  activeDescendant: string | undefined;
  format: Choice;
  formatButtonRef?: React.RefObject<HTMLButtonElement | null>;
  /** The app the picker was opened from, when Ibis knows it. */
  app?: string | null;
  formatScope?: string | null;
  onFormatOpenChange?: (open: boolean, reason?: MenuCloseReason) => void;
  onceOnly?: boolean;
  onToggleOnce?: () => void;
  onAutomatic?: () => void;
  onDefault?: () => void;
  formatDisabled?: boolean;
  showFormat: boolean;
  /** Whether formatted citation styles can be used now. */
  styles: boolean;
  onFormatChange: (choice: Choice) => void;
  onFormatCommand: (command: FormatCommand) => ToolbarMenuCommand;
}) {
  return (
    <header className="search-bar" data-tauri-drag-region title={pickerText.dragSearchBar}>
      {mark ? <svg className="search-mark" aria-hidden="true" viewBox={ibisMark.viewBox} fill="currentColor" dangerouslySetInnerHTML={{ __html: ibisMark.body }} />
        : <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor">
          <circle cx="10.5" cy="10.5" r="6.5" />
          <path d="m16 16 5 5" />
        </svg>}
      <label className="visually-hidden" htmlFor="search">
        {pickerText.searchLabel}
      </label>
      <div className="search-field" title=""
        onKeyDownCapture={(e) => { if (composing(e)) e.stopPropagation(); }}
        onKeyUpCapture={(e) => { if (composing(e)) e.stopPropagation(); }}>
        <input
          id="search"
          type="search"
          placeholder={pickerText.searchPlaceholder}
          role="combobox"
          aria-autocomplete="list"
          aria-expanded={expanded}
          aria-controls="results"
          aria-activedescendant={activeDescendant}
          ref={inputRef}
          autoFocus
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          onPaste={(e) => {
            const text = e.clipboardData.getData("text/plain");
            if (!/[\r\n\t]/.test(text)) return;
            // Search inputs strip line breaks but retain tabs. Keep whitespace
            // inside citation arguments; punctuation would change their keys.
            e.preventDefault();
            const field = e.currentTarget;
            const start = field.selectionStart ?? query.length;
            const end = field.selectionEnd ?? start;
            const pasted = text.replace(/[\r\n]+/g, "\t");
            const next = query.slice(0, start) + pasted + query.slice(end);
            onQueryChange(next);
            requestAnimationFrame(() => {
              if (field.value === next) field.setSelectionRange(start + pasted.length, start + pasted.length);
            });
          }}
          onKeyDown={onSearchKeyDown}
        />
        {query !== "" && <Button variant="icon" glyph="close" className="search-clear" aria-label={pickerText.clearSearch}
          onPress={() => { onQueryChange(""); inputRef.current?.focus(); }} />}
      </div>
      {showFormat && <FormatMenu format={format} buttonRef={formatButtonRef} app={app} scope={formatScope} onOpenChange={onFormatOpenChange} onceOnly={onceOnly} onToggleOnce={onToggleOnce} onAutomatic={onAutomatic} onDefault={onDefault} disabled={formatDisabled} styles={styles} onChange={onFormatChange} searchRef={inputRef} onCommand={onFormatCommand} />}
      <ToggleButton variant="icon" className="pin-button" aria-label={pickerText.keepOpen} aria-keyshortcuts={ariaKeys(keysFor("pin"), currentPlatform() === "macos")}
        isSelected={pinned} onChange={onTogglePin}>
        <svg aria-hidden="true" viewBox="0 0 24 24" fill={pinned ? "currentColor" : "none"} stroke="currentColor">
          <path d="M9 4h6l-1 5 4 4H6l4-4z" /><path d="M12 13v7" />
        </svg>
      </ToggleButton>
    </header>
  );
}
