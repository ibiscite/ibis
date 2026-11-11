import type { FormatChoice } from "../bindings";

/** The engine's answer when nothing is chosen for an app (decision 0042). */
export const noFormatChosen: FormatChoice = {
  output: { kind: "key", syntax: "pandoc", command: null }, id: "pandoc", from: "suggestion", kind: "keys",
};
