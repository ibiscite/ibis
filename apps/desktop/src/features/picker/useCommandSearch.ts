import { useMemo, useState } from "react";
import { composing } from "../../composition";
import { pickerText } from "../../messages";
import { COMMAND_PREFIX, matchCommands, pickerCommands, type PickerCommandEntry } from "./commands";
import type { PickerCommand } from "./shortcuts";

/** ">" leading the search turns the list into the picker's commands, as in
 * Raycast: ↑ ↓ move, Enter or a click runs one and clears the search, Esc steps
 * back as it always does. Commands run through the same path as their keys. */
export function useCommandSearch({ query, run, setQuery, source, changeLast }: {
  query: string; run: (command: PickerCommand) => void; setQuery: (value: string) => void;
  source: string | null; changeLast: boolean;
}) {
  const mode = query.startsWith(COMMAND_PREFIX);
  const list = useMemo(() => mode ? matchCommands(query, pickerCommands({ source, changeLast })) : [], [mode, query, source, changeLast]);
  // The highlight belongs to one typed query; a new one starts at the top.
  const [highlight, setHighlight] = useState({ query, index: 0 });
  const active = highlight.query === query ? Math.min(highlight.index, Math.max(list.length - 1, 0)) : 0;
  const runCommand = (command: PickerCommandEntry) => { setQuery(""); run(command.id); };
  const keys = (event: KeyboardEvent) => {
    if (!mode || composing(event)) return false;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setHighlight({ query, index: Math.min(Math.max(active + step, 0), Math.max(list.length - 1, 0)) });
      return true;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      if (!event.repeat && list[active]) runCommand(list[active]);
      return true;
    }
    return false;
  };
  const chosen = mode ? list[active] : undefined;
  return {
    mode, list, active, run: runCommand, keys,
    // With nothing to run, the footer offers nothing rather than a dead action.
    footer: mode ? { label: chosen ? pickerText.runCommand(chosen.title) : null, run: () => { if (chosen) runCommand(chosen); } } : null,
  };
}
