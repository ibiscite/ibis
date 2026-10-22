import type { RefObject } from "react";
import { COPY_FORMATS } from "../../api";
import { FORMATTED_STYLES } from "../../format/formatted";
import { STYLE_NAMES, STYLE_SAMPLES } from "../../format/styles";
import { copyFormatMenu, pickerText } from "../../messages";
import { ToolbarMenu, type ToolbarMenuCommand, type ToolbarMenuSection } from "../../ui/ToolbarMenu";
import { choiceName, type Choice } from "./choice";
import { isKeysShortcut, isMenuShortcut, isPinShortcut, isPreviewShortcut, isQuitShortcut } from "./shortcuts";

export type FormatCommand = "more" | "keys" | "pin" | "preview" | "quit";

/** The copy format menu: key syntaxes and citation styles, each with a real
 * sample. Styles stay listed but unavailable while the engine is not offered.
 * Focus goes back to the search box after a pick, Escape or Tab. */
export function FormatMenu({ format, styles, onChange, searchRef, onCommand }: {
  format: Choice;
  /** Whether formatted citation styles can be used now. */
  styles: boolean;
  onChange: (choice: Choice) => void;
  searchRef: RefObject<HTMLElement | null>;
  onCommand: (command: FormatCommand) => ToolbarMenuCommand;
}) {
  const sections: ToolbarMenuSection[] = [
    { id: "key-formats", label: pickerText.keyFormats, items: COPY_FORMATS.map(({ value }) => ({ id: value, ...copyFormatMenu[value] })) },
    { id: "citation-styles", label: pickerText.formattedStyles, items: FORMATTED_STYLES.map(({ id }) => (
      { id: `style:${id}`, label: STYLE_NAMES[id], sample: STYLE_SAMPLES[id], prose: true, disabled: !styles })) },
  ];
  return <ToolbarMenu label={pickerText.copyAs} value={choiceName(format)} sections={sections} selectedKey={format}
    filterLabel={pickerText.filterFormats} onKeyCommand={(event) => {
      const command = isMenuShortcut(event) ? "more" : isPinShortcut(event) && !event.repeat ? "pin" : isKeysShortcut(event) ? "keys"
        : isPreviewShortcut(event) && !event.repeat ? "preview" : isQuitShortcut(event) ? "quit" : null;
      return command ? onCommand(command) : undefined;
    }} onAction={(key) => onChange(key as Choice)} returnFocusRef={searchRef} />;
}
