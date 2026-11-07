import type React from "react";
import type { MouseEvent } from "react";
import { IbisMark } from "../../ui/IbisMark";
import { IconButton } from "../../ui/IconButton";
import { SearchBar, SearchField } from "../../ui/SearchBar";
import { say } from "../../i18n";
import { ariaKeys, keysFor } from "../../keys";
import { currentPlatform } from "../../platform";
import type { OutputId } from "../../format/outputs";
import { FormatMenu } from "./FormatMenu";
import type { MenuCloseReason } from "../../ui/ToolbarMenu";
import type { AppPaste } from "../../api";
import type { FormatPick, FormatSource } from "./useDestination";

/** The search bar: the search box and the copy format. */
export function PickerHeader({ hidden = false, onResetPosition, inputRef, query, pageToken, onQueryChange, expanded, activeDescendant, format, formatButtonRef, app = null, formatSource = "default", formatPick = "default", onFormatOpenChange, pasteChoices, onPasteChange, formatDisabled = false, showFormat, styles, onFormatChange, pinned, onTogglePin, writingProject, onWritingProjects }: {
  /** Welcome has no search yet; the field stays mounted so focus can return to it. */
  hidden?: boolean;
  inputRef: React.RefObject<HTMLInputElement | null>;
  pinned: boolean;
  onTogglePin: () => void;
  query: string;
  pageToken?: React.ReactNode;
  onQueryChange: (query: string) => void;
  /** Whether the result list has rows. */
  expanded: boolean;
  /** The id of the highlighted row, when there is one. */
  activeDescendant: string | undefined;
  format: OutputId;
  formatButtonRef?: React.RefObject<HTMLButtonElement | null>;
  /** The app the picker was opened from, when Ibis knows it. */
  app?: string | null;
  formatSource?: FormatSource;
  formatPick?: FormatPick;
  writingProject?: { name: string } | null;
  onWritingProjects?: () => void;
  onFormatOpenChange?: (open: boolean, reason?: MenuCloseReason) => void;
  pasteChoices?: boolean;
  onPasteChange?: (paste: AppPaste) => void;
  formatDisabled?: boolean;
  showFormat: boolean;
  /** Whether formatted citation styles can be used now. */
  styles: boolean;
  onFormatChange: (choice: OutputId) => void;
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
    <SearchBar hidden={hidden} title={say("picker-drag-search-bar")}
      onMouseDown={emptyArea} onMouseUp={(event) => { if (emptyArea(event)) onResetPosition?.(); }}>
      <IbisMark className="search-mark" size="search" />
      <SearchField
        id="search"
        label={say("picker-search-label")}
        clearLabel={say("picker-clear-search")}
        onClear={() => { onQueryChange(""); inputRef.current?.focus(); }}
        trailing={pageToken}
        placeholder={say("picker-search-placeholder")}
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
      />
      {showFormat && <FormatMenu format={format} buttonRef={formatButtonRef} app={app} source={formatSource} pick={formatPick} onOpenChange={onFormatOpenChange} pasteChoices={pasteChoices} onPasteChange={onPasteChange} disabled={formatDisabled} styles={styles} onChange={onFormatChange} searchRef={inputRef} writingProject={writingProject} onWritingProjects={onWritingProjects} />}
      <IconButton icon="pin" size="large" className="pin-button" aria-label={say("picker-keep-open")} aria-keyshortcuts={ariaKeys(keysFor("pin"), currentPlatform() === "macos")}
        isSelected={pinned} onChange={onTogglePin}/>
    </SearchBar>
  );
}
