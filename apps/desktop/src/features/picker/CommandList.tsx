import { Status } from "../../ui/Status";
import { Icon } from "../../icons";
import { keyLabel } from "../../keys";
import { say } from "../../i18n";
import { currentPlatform } from "../../platform";
import { FrameHeading } from "../../ui/Frame";
import { Option } from "../../ui/Option";
import type { PickerCommandEntry } from "./commands";

/** ">" in the search: the commands as one-line result rows, highlighted from the
 * search as references are, so ↑ ↓ and Enter work the same and the pointer can
 * click. A command's description only repeats its title, so it is matched and
 * spoken, never drawn. */
export function CommandList({ commands, active, hoverAllowed, onRun }: {
  commands: readonly PickerCommandEntry[]; active: number; hoverAllowed: boolean;
  onRun: (command: PickerCommandEntry) => void;
}) {
  const mac = currentPlatform() === "macos";
  // The heading sits in the frame's line, as the results' does, where a notice can stand in for it.
  if (!commands.length) return <FrameHeading><Status as="div" className="ui-list-heading"><span>{say("picker-no-commands")}</span></Status></FrameHeading>;
  return <>
    <FrameHeading><div className="ui-list-heading" aria-hidden="true"><span>{say("picker-commands")}</span></div></FrameHeading>
    <ul id="results" role="listbox" aria-label={say("picker-commands")}>
      {commands.map((command, i) => <Option key={command.id} id={`command-${i}`} aria-label={command.title} className="command-row" role="option" aria-selected={i === active}
        aria-keyshortcuts={command.keys || undefined} aria-description={command.about} data-density="compact" data-active={i === active || undefined}
        hoverAllowed={hoverAllowed} onClick={() => onRun(command)} icon={<span className="result-tile"><Icon name={command.icon} size="tile" /></span>}>
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
