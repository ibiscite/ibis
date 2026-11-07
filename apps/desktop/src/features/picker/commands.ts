// Every picker command by name, for ">" in the search (docs/design/picker.md,
// Command search). Titles are the words the menus use and keys come from KEY_TABLE, so a
// command is written once and reads the same in More, the keys sheet and here.
import type { IconName } from "../../icons";
import { keysFor } from "../../keys";
import { keyText, pickerText } from "../../messages";
import { isMac } from "../../platform";
import type { PickerCommand } from "./shortcuts";

/** Typed first in the search, it turns the list into commands, as in Raycast. */
export const COMMAND_PREFIX = ">";

export type PickerCommandEntry = { id: PickerCommand; title: string; about: string; keys: string; icon: IconName };

/** The glyph each command shows on the tile column, from the one icon set. */
const ICONS: Record<PickerCommand, IconName> = {
  format: "swap", source: "file", revealSource: "find", chooseSource: "library", changeLast: "write",
  pin: "pin", keys: "shortcuts", settings: "general", quit: "close", more: "chevrons",
};

/** The commands available now. `source` names the library's own action
 * ("Open thesis.bib", "Show in Zotero"); it is left out when there is none. */
export function pickerCommands({ source, changeLast }: { source: string | null; changeLast: boolean }): PickerCommandEntry[] {
  const titled: [PickerCommand, string | null][] = [
    ["format", pickerText.changeFormat],
    ["source", source],
    ["revealSource", source && pickerText.showSourceFolder(isMac)],
    ["chooseSource", pickerText.chooseLibraryMenu],
    ["changeLast", changeLast ? pickerText.changeLastCitation : null],
    ["pin", pickerText.keepOpenMenu],
    ["keys", keyText.title],
    ["settings", pickerText.settings],
    ["quit", pickerText.quitLabel],
  ];
  return titled.flatMap(([id, title]) => title ? [{ id, title, about: keyText.actions[id], keys: keysFor(id), icon: ICONS[id] }] : []);
}

/** Commands whose title or description holds every typed word, in table order. */
export function matchCommands(query: string, commands: readonly PickerCommandEntry[]): PickerCommandEntry[] {
  const words = query.slice(COMMAND_PREFIX.length).toLocaleLowerCase().split(/\s+/).filter(Boolean);
  return commands.filter((command) => {
    const text = `${command.title} ${command.about}`.toLocaleLowerCase();
    return words.every((word) => text.includes(word));
  });
}
