import type React from "react";
import { IbisMark } from "../../ui/IbisMark";
import { SearchBar, SearchField } from "../../ui/SearchBar";
import { say } from "../../i18n";
import type { OutputId } from "../../format/outputs";
import { FormatMenu, type FormatExample, type FormatPreview } from "./FormatMenu";
import type { MenuCloseReason } from "./FormatMenu";
import type { AppPaste } from "../../api";
import type { FormatPick, FormatSource } from "./useDestination";

/** The search bar: the search box and the copy format. */
export function PickerHeader({ hidden = false, onResetPosition, inputRef, query, pageToken, onQueryChange, expanded, activeDescendant, format, formatButtonRef, app = null, formatSource = "default", formatPick = "default", onFormatOpenChange, pasteChoices, onPasteChange, formatDisabled = false, showFormat, styles, onFormatChange, yours, preview, example, writing, onWritings, usualFormat, onUseUsual, formatLabel, delivery }: {
  /** Welcome has no search yet; the field stays mounted so focus can return to it. */
  hidden?: boolean;
  inputRef: React.RefObject<HTMLInputElement | null>;
  /** Your own format, the one the chooser offers to return to. */
  yours?: OutputId;
  /** The chosen references as another format would paste them. */
  preview?: FormatPreview | null;
  example?: FormatExample | null;
  query: string;
  pageToken?: React.ReactNode;
  onQueryChange: (query: string) => void;
  /** Whether the result list has rows. */
  expanded: boolean;
  /** The id of the highlighted row, when there is one. */
  activeDescendant: string | undefined;
  format: OutputId;
  formatLabel?: string;
  delivery?: ReturnType<typeof import("./enterLabel").citationDelivery>;
  formatButtonRef?: React.RefObject<HTMLButtonElement | null>;
  /** The app the picker was opened from, when Ibis knows it. */
  app?: string | null;
  formatSource?: FormatSource;
  formatPick?: FormatPick;
  writing?: { name: string } | null;
  onWritings?: () => void;
  usualFormat?: OutputId | null;
  onUseUsual?: () => void;
  onFormatOpenChange?: (open: boolean, reason?: MenuCloseReason) => void;
  pasteChoices?: boolean;
  onPasteChange?: (paste: AppPaste) => void;
  formatDisabled?: boolean;
  showFormat: boolean;
  /** Whether formatted citation styles can be used now. */
  styles: boolean;
  onFormatChange: (choice: OutputId) => void;
  /** Double-clicking the bird or the bar's empty room returns Ibis to its place. */
  onResetPosition?: () => void;
}) {
  return (
    <SearchBar hidden={hidden} title={say("picker-drag-search-bar")} mark={<IbisMark className="search-mark" size="search" />}
      onResetPosition={onResetPosition}>
      <SearchField
        id="search"
        label={say("picker-search-label")}
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
      {showFormat && (delivery?.editorFormat ? <span className="search-format-label">{delivery.editorFormat} · {delivery.label}</span> : <FormatMenu format={format} buttonRef={formatButtonRef} app={app} source={formatSource} pick={formatPick} onOpenChange={onFormatOpenChange} pasteChoices={pasteChoices} onPasteChange={onPasteChange} disabled={formatDisabled} styles={styles} onChange={onFormatChange} searchRef={inputRef} writing={writing} onWritings={onWritings} usualFormat={usualFormat} onUseUsual={onUseUsual} formatLabel={formatLabel} yours={yours} preview={preview} example={example} deliveryLabel={app ? delivery?.label : undefined} />)}
    </SearchBar>
  );
}
