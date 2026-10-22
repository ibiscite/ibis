import type React from "react";
import { pickerText } from "../../messages";
import type { Choice } from "./choice";
import { FormatMenu, type FormatCommand } from "./FormatMenu";
import type { ToolbarMenuCommand } from "../../ui/ToolbarMenu";

/** The search bar: the search box, the copy format and the pin. It is also
 * the window's drag region; the editable box itself is not. */
export function PickerHeader({ inputRef, query, onQueryChange, onSearchKeyDown, expanded, activeDescendant, format, styles, onFormatChange, onFormatCommand, pinned, onTogglePin }: {
  inputRef: React.RefObject<HTMLInputElement | null>;
  query: string;
  onQueryChange: (query: string) => void;
  onSearchKeyDown: (e: React.KeyboardEvent<HTMLInputElement>) => void;
  /** Whether the result list has rows. */
  expanded: boolean;
  /** The id of the highlighted row, when there is one. */
  activeDescendant: string | undefined;
  format: Choice;
  /** Whether formatted citation styles can be used now. */
  styles: boolean;
  onFormatChange: (choice: Choice) => void;
  onFormatCommand: (command: FormatCommand) => ToolbarMenuCommand;
  pinned: boolean;
  onTogglePin: () => void;
}) {
  return (
    <header className="search-bar" data-tauri-drag-region title={pickerText.dragSearchBar}>
      <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor">
        <circle cx="10.5" cy="10.5" r="6.5" />
        <path d="m16 16 5 5" />
      </svg>
      <label className="visually-hidden" htmlFor="search">
        {pickerText.searchLabel}
      </label>
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
        onKeyDown={onSearchKeyDown}
      />
      <FormatMenu format={format} styles={styles} onChange={onFormatChange} searchRef={inputRef} onCommand={onFormatCommand} />
      <button
        type="button"
        className="pin-button"
        aria-pressed={pinned}
        aria-label={pickerText.pin}
        title={pinned ? pickerText.unpinTitle : pickerText.pinTitle}
        onClick={onTogglePin}
      >
        <svg aria-hidden="true" viewBox="0 0 24 24" fill={pinned ? "currentColor" : "none"} stroke="currentColor">
          <path d="M9 4h6l-1 5 4 4H6l4-4z" />
          <path d="M12 13v7" />
        </svg>
      </button>
    </header>
  );
}
