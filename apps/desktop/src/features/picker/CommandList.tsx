import { Icon } from "../../icons";
import { keyLabel } from "../../keys";
import { say } from "../../i18n";
import { currentPlatform } from "../../platform";
import { Option } from "../../ui/Option";
import type { PickerCommandEntry } from "./commands";
import "./CommandList.css";

/** ">" in the search: the commands as one-line result rows, highlighted from the
 * search as references are, so ↑ ↓ and Enter work the same and the pointer can
 * click. A command's description only repeats its title, so it is matched and
 * spoken, never drawn. */
export function CommandList({ commands, active, hoverAllowed, onRun }: {
  commands: readonly PickerCommandEntry[]; active: number; hoverAllowed: boolean;
  onRun: (command: PickerCommandEntry) => void;
}) {
  const mac = currentPlatform() === "macos";
  if (!commands.length) return <div className="results-heading" role="status"><span>{say("picker-no-commands")}</span></div>;
  return <>
    <div className="results-heading" aria-hidden="true"><span>{say("picker-commands")}</span></div>
    <ul id="results" role="listbox" aria-label={say("picker-commands")}>
      {commands.map((command, i) => <Option key={command.id} id={`command-${i}`} className="command-row" role="option" aria-selected={i === active}
        aria-keyshortcuts={command.keys || undefined} aria-description={command.about} data-density="compact" data-active={i === active || undefined}
        hoverAllowed={hoverAllowed} onClick={() => onRun(command)}>
        <span className="result-tile"><Icon name={command.icon} /></span>
        <span className="result-body">
          <span className="result-line">
            <strong className="result-title">{command.title}</strong>
            {command.keys && <span className="result-year" aria-hidden="true">{keyLabel(command.keys, mac)}</span>}
          </span>
        </span>
      </Option>)}
    </ul>
  </>;
}
