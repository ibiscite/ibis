import type { RefObject } from "react";
import { ariaKeys, keysFor } from "../../keys";
import { isMac } from "../../platform";
import { COPY_FORMATS } from "../../api";
import { FORMATTED_STYLES } from "../../format/styles";
import { STYLE_SAMPLES } from "../../format/styles";
import { copyFormatMenu, pickerText } from "../../messages";
import { ToolbarMenu, type ToolbarMenuCommand, type ToolbarMenuSection, type MenuCloseReason } from "../../ui/ToolbarMenu";
import { outputName, type Choice } from "./choice";
import { isKeysShortcut, isMenuShortcut, isPinShortcut, isPreviewShortcut, isQuitShortcut } from "./shortcuts";

const ONCE = "just-once";
const AUTOMATIC = "automatic";
const DEFAULT = "global-default";

export type FormatCommand = "more" | "keys" | "pin" | "preview" | "quit";

/** The copy format menu: key syntaxes and citation styles, each with a real
 * sample. Styles stay listed but unavailable while the engine is not offered.
 * Focus goes back to the search box after a pick, Escape or Tab. */
export function FormatMenu({ format, buttonRef, app, scope, onOpenChange, onceOnly, onToggleOnce, onAutomatic, onDefault, disabled, styles, onChange, searchRef, onCommand }: {
  format: Choice;
  buttonRef?: RefObject<HTMLButtonElement | null>;
  /** The app the picker was opened from, named beside the format. */
  app: string | null;
  scope?: string | null;
  onOpenChange?: (open: boolean, reason?: MenuCloseReason) => void;
  onceOnly: boolean;
  onToggleOnce: () => void;
  onAutomatic: () => void;
  onDefault: () => void;
  disabled: boolean;
  /** Whether formatted citation styles can be used now. */
  styles: boolean;
  onChange: (choice: Choice) => void;
  searchRef: RefObject<HTMLElement | null>;
  onCommand: (command: FormatCommand) => ToolbarMenuCommand;
}) {
  const sections: ToolbarMenuSection[] = [
    { id: "scope", label: app ? pickerText.forApp(app) : pickerText.copyAs, items: [
      { id: ONCE, label: pickerText.justOnce, checked: onceOnly },
      ...app ? [{ id: AUTOMATIC, label: pickerText.automatic, command: true }] : [],
      { id: DEFAULT, label: pickerText.globalDefault, command: true },
    ] },
    { id: "key-formats", label: pickerText.keyFormats, items: COPY_FORMATS.map(({ value }) => ({ id: value, ...copyFormatMenu[value] })) },
    { id: "citation-styles", label: pickerText.formattedStyles, items: FORMATTED_STYLES.map(({ id }) => (
      { id: `style:${id}`, label: outputName(`style:${id}`), sample: STYLE_SAMPLES[id], prose: true, disabled: !styles })) },
  ];
  const name = outputName(format);
  return <><span role="status" className="visually-hidden">{scope}</span><ToolbarMenu buttonRef={buttonRef} onOpenChange={onOpenChange} buttonShortcut={ariaKeys(keysFor("format"), isMac)} disabled={disabled} label={pickerText.copyAs} value={scope ?? (app ? pickerText.formatForApp(name, app) : name)} sections={sections} selectedKey={format}
    filterLabel={pickerText.filterFormats} onKeyCommand={(event) => {
      const command = isMenuShortcut(event) ? "more" : isPinShortcut(event) && !event.repeat ? "pin" : isKeysShortcut(event) ? "keys"
        : isPreviewShortcut(event) && !event.repeat ? "preview" : isQuitShortcut(event) ? "quit" : null;
      return command ? onCommand(command) : undefined;
    }} onAction={(key) => key === ONCE ? onToggleOnce() : key === AUTOMATIC ? onAutomatic() : key === DEFAULT ? onDefault() : onChange(key as Choice)} returnFocusRef={searchRef} /></>;
}
