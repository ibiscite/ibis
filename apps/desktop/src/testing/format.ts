import type { FormatChoice, FormatFrom } from "../bindings";
import type { PickerDestination } from "../api";

/** The engine's answer when nothing is chosen for an app (decision 0042). */
export const noFormatChosen: FormatChoice = {
  output: { kind: "key", syntax: "pandoc", command: null }, id: "pandoc", from: "suggestion", kind: "keys",
};

/** An explicit engine answer for UI scenarios; these tests exercise its id
 * and source, while Rust tests exercise how it was resolved. */
export const formatAnswer = (id: string, from: FormatFrom = "app"): FormatChoice => ({ ...noFormatChosen, id, from });

/** A complete synthetic native destination with an explicit engine answer. */
export function fixtureDestination(destination: Partial<PickerDestination>): PickerDestination {
  return { app: null, app_id: null, app_identity: null, unreadable: false,
    can_paste: false, paste: null, paste_question: false, can_choose_paste: false,
    ...destination, find_open: destination.find_open ?? false, usual_format: destination.usual_format ?? "apa",
    format: destination.format ?? noFormatChosen };
}
