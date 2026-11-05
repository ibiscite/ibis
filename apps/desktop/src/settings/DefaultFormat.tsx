import { useRef } from "react";
import type { PickerPreferences } from "../bindings";
import { defaultOutput, keyOutputs, listedOutput, outputName, styleOutputs } from "../format/outputs";
import { useInterfaceMessages } from "../i18n";
import { ToolbarMenu } from "../ui/ToolbarMenu";

/** The same Ibis popup, checks, filtering and keyboard behaviour as the picker,
 * listing the engine's outputs by their one name. */
export function DefaultFormat({ preferences, change }: {
  preferences: PickerPreferences;
  change: (patch: Partial<PickerPreferences>) => void;
}) {
  const { text } = useInterfaceMessages();
  const button = useRef<HTMLButtonElement>(null);
  const sections = [
    { id: "default-key-formats", label: text("citations-key-formats"), items: keyOutputs.map((output) => ({ id: output.id, label: output.name })) },
    { id: "default-citation-styles", label: text("citations-style-formats"), items: styleOutputs
      .map((output) => ({ id: output.id, label: output.name, detail: output.description ?? undefined })) },
  ];
  const value = listedOutput(preferences.output) ?? defaultOutput;
  return <ToolbarMenu buttonRef={button} returnFocusRef={button} label={text("citations-default")}
    value={outputName(value)} selectedKey={value} sections={sections} filterLabel={text("citations-filter-formats")}
    onAction={(choice) => {
      const output = listedOutput(choice);
      if (output) change({ output });
    }} />;
}
