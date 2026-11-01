import { choiceName, copyLabel, styleOf, type Choice } from "./choice";
import { copiesEntry } from "../../format/styles";
import { pickerText } from "../../messages";

/** Whether Enter pastes rather than copies. The footer and the keys sheet both ask this. */
export function enterPastes(app: string | null, canPaste: boolean, stayOpen: boolean): app is string {
  return canPaste && app !== null && app !== "" && !stayOpen;
}

/** One phrase for the action and the output. Numeric styles still name entries. */
export function enterLabel(format: Choice, count: number, app: string | null, canPaste: boolean, stayOpen: boolean, pastedList: boolean) {
  const style = styleOf(format);
  const entries = style !== null && copiesEntry(style);
  const output = entries ? pickerText.entryOutput(choiceName(format), count) : choiceName(format);
  if (enterPastes(app, canPaste, stayOpen)) return pickerText.pasteInto(app, output);
  if (entries) return copyLabel(format, count);
  return pastedList ? `${pickerText.copyCitations(count)} · ${choiceName(format)}` : copyLabel(format, count);
}
