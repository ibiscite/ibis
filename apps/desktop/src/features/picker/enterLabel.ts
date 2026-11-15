import { copyLabel } from "./choice";
import { outputName, writesEntries, type OutputId } from "../../format/outputs";
import { say } from "../../i18n";

/** Whether Enter pastes rather than copies. The footer and the keys sheet both ask this. */
export function enterPastes(app: string | null, canPaste: boolean, stayOpen: boolean): app is string {
  return canPaste && app !== null && app !== "" && !stayOpen;
}

/** One destination for the format label, action and nearby delivery words. */
export function citationDelivery(app: string | null, canPaste: boolean, stayOpen: boolean, editorFormat: string | null) {
  const pastes = !editorFormat && enterPastes(app, canPaste, stayOpen);
  const label = editorFormat ? say("picker-delivery-editor") : pastes ? app! : say("picker-delivery-clipboard");
  return { pastes, label, mode: pastes ? say("picker-delivery-paste") : label, editorFormat };
}

/** One phrase for the action and the output. Numeric styles still name entries. */
export function enterLabel(format: OutputId, count: number, app: string | null, canPaste: boolean, stayOpen: boolean, pastedList: boolean) {
  const entries = writesEntries(format);
  const output = entries ? say("picker-entry-output", { style: outputName(format), count: count }) : outputName(format);
  if (enterPastes(app, canPaste, stayOpen)) return say("picker-paste-into", { app, output });
  if (entries) return copyLabel(format, count);
  return pastedList ? `${say("picker-copy-citations", { count: count })} · ${outputName(format)}` : copyLabel(format, count);
}
