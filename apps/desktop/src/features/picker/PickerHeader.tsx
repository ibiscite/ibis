import type { SearchIcon } from "../../bindings";
import type React from "react";
import type { MouseEvent } from "react";
import { IbisMark } from "../../ui/IbisMark";
import { Button, ToggleButton } from "../../ui/Button";
import { composing } from "./shortcuts";
import { pickerText } from "../../messages";
import { ariaKeys, keysFor } from "../../keys";
import { currentPlatform } from "../../platform";
import type { Choice } from "./choice";
import { FormatMenu, type FormatCommand } from "./FormatMenu";
import type { MenuCloseReason, ToolbarMenuCommand } from "../../ui/ToolbarMenu";
import type { AppPaste } from "../../api";

/** The search bar: the search box and the copy format. It is also
 * the window's drag region; the editable box itself is not. */
export function PickerHeader({ onResetPosition, inputRef, query, pageToken, onQueryChange, onSearchKeyDown, expanded, activeDescendant, format, formatButtonRef, app = null, formatScope, onFormatOpenChange, onceOnly = false, onToggleOnce = () => undefined, onAutomatic = () => undefined, onDefault = () => undefined, pasteChoices, onPasteChange, formatDisabled = false, showFormat, styles, onFormatChange, onFormatCommand, pinned, onTogglePin, icon = "magnifier" }: {
  inputRef: React.RefObject<HTMLInputElement | null>;
  pinned: boolean;
  /** What leads the search field, a person's choice in Look. */
  icon?: SearchIcon;
  onTogglePin: () => void;
  query: string;
  pageToken?: React.ReactNode;
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
  pasteChoices?: boolean;
  onPasteChange?: (paste: AppPaste) => void;
  formatDisabled?: boolean;
  showFormat: boolean;
  /** Whether formatted citation styles can be used now. */
  styles: boolean;
  onFormatChange: (choice: Choice) => void;
  onFormatCommand: (command: FormatCommand) => ToolbarMenuCommand;
  /** Double-clicking the bar's empty area returns Ibis to its place. */
  onResetPosition?: () => void;
}) {
  // The drag script on the document would zoom the window instead.
  const emptyArea = (event: MouseEvent<HTMLElement>) => {
    const twice = event.detail === 2 && event.target === event.currentTarget;
    if (twice) event.nativeEvent.stopImmediatePropagation();
    return twice;
  };
  return (
    <header className="search-bar" data-icon={icon} data-tauri-drag-region title={pickerText.dragSearchBar}
      onMouseDown={emptyArea} onMouseUp={(event) => { if (emptyArea(event)) onResetPosition?.(); }}>
      {icon === "ibis" && <IbisMark className="search-mark" size="search" />}
      {icon !== "ibis" && <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor">
        <path d="M10.5 4a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM16 16l5 5" />
      </svg>}
      <label className="visually-hidden" htmlFor="search">
        {pickerText.searchLabel}
      </label>
      <div className="search-field" title=""
        onKeyDownCapture={(e) => { if (composing(e)) e.stopPropagation(); }}
        onKeyUpCapture={(e) => { if (composing(e)) e.stopPropagation(); }}>
        <div className="query-input">
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
        {pageToken}
      </div>
      {showFormat && <FormatMenu format={format} buttonRef={formatButtonRef} app={app} scope={formatScope} onOpenChange={onFormatOpenChange} onceOnly={onceOnly} onToggleOnce={onToggleOnce} onAutomatic={onAutomatic} onDefault={onDefault} pasteChoices={pasteChoices} onPasteChange={onPasteChange} disabled={formatDisabled} styles={styles} onChange={onFormatChange} searchRef={inputRef} onCommand={onFormatCommand} />}
      <ToggleButton variant="icon" className="pin-button" aria-label={pickerText.keepOpen} aria-keyshortcuts={ariaKeys(keysFor("pin"), currentPlatform() === "macos")}
        isSelected={pinned} onChange={onTogglePin}>
        <svg aria-hidden="true" viewBox="0 0 24 24" fill={pinned ? "currentColor" : "none"} stroke={pinned ? "none" : "currentColor"}>
          <path d={pinned ? "M8.5 3.25h7L14.2 8.7 18 12.5v1.25h-5.25V20.5h-1.5v-6.75H6V12.5l3.8-3.8z" : "M9 4h6l-1 5 4 4H6l4-4zM12 13v7"} />
        </svg>
      </ToggleButton>
    </header>
  );
}
