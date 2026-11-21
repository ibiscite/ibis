// The glyph each picker command shows on the tile column in More (⌘K), from the one icon set.
import type { IconName } from "../../icons";
import type { PickerCommand } from "./shortcuts";

export const COMMAND_ICONS: Record<PickerCommand, IconName> = {
  format: "format", source: "file", revealSource: "folder-search", chooseSource: "library-big", changeLast: "write",
  pin: "pin", keys: "keyboard", settings: "settings", quit: "quit", more: "chevrons", inspector: "hash",
};
