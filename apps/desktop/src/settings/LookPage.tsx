import type { Accent, Appearance, PickerPreferences, WindowSize } from "../bindings";
import { useInterfaceMessages } from "../i18n";
import { currentPlatform } from "../platform";
import { PictureChoice } from "../ui/PictureChoice";
import { Segmented } from "../ui/Segmented";
import { SettingsGroup as Group, SettingsRow as Row } from "../ui/SettingsGroup";
import { PickerPicture } from "./PickerPicture";

const ACCENTS: readonly Accent[] = ["system", "ibis"];
const APPEARANCES: readonly Appearance[] = ["system", "light", "dark"];
type Feedback = (id: string) => { highlighted: boolean; problem?: string };
/** Each size's window in CSS pixels (src-tauri sizing.rs); Custom has no set shape, so it shows the default. */
const WINDOWS = { small: [640, 400], medium: [720, 480], large: [840, 600], custom: [720, 480] } as const;
export function LookPage({ preferences: p, change, feedback, windowSize, chooseWindowSize }: {
  preferences: PickerPreferences; change: (patch: Partial<PickerPreferences>) => void; feedback: Feedback;
  windowSize: WindowSize | null; chooseWindowSize: (size: WindowSize) => void;
}) {
  const platform = currentPlatform();
  const { text } = useInterfaceMessages();
  const appearance = p.appearance ?? "system", accent = p.accent ?? "system";
  return <>
    <Group id="look-appearance" title={text("look-appearance")}>
      <Row bare id="appearance" label={text("look-appearance")} {...feedback("appearance")}>
        <PictureChoice label={text("look-appearance")} value={appearance}
          options={APPEARANCES.map((value) => ({ value, label: text(`look-${value}`), picture: <PickerPicture density={p.density} appearance={value === "system" ? "light" : value} split={value === "system"} /> }))}
          onChange={(appearance) => change({ appearance })} />
      </Row>
    </Group>
    <Group id="look-accent">
      <Row id="accent" label={text("look-accent")} {...feedback("accent")}>
        <Segmented label={text("look-accent")} value={accent}
          options={ACCENTS.map((value) => ({ value, label: text(`look-accent-${value}`, { platform }) }))}
          onChange={(accent) => change({ accent })} />
      </Row>
    </Group>
    <Group id="look-rows" title={text("general-rows")}>
      <Row bare id="rows" label={text("general-rows")} {...feedback("rows")}>
        <PictureChoice strips label={text("general-rows")} value={p.density}
          options={(["compact", "comfortable"] as const).map((value) => ({ value, label: text(`general-density-${value}`), note: text(`look-density-${value}`), picture: <PickerPicture rowsOnly density={value} /> }))}
          onChange={(density) => change({ density })} />
      </Row>
    </Group>
    <Group id="look-size" title={text("look-window-size")}>
      <Row bare id="window-size" label={text("look-window-size")} {...feedback("window-size")}>
        <PictureChoice label={text("look-window-size")} value={windowSize ?? "medium"} isDisabled={windowSize === null}
          options={(windowSize === "custom" ? ["small", "medium", "large", "custom"] as const : ["small", "medium", "large"] as const)
            .map((value) => ({ value, label: text(`look-window-${value}`), picture: <PickerPicture density={p.density} frame={WINDOWS[value]} /> }))}
          onChange={chooseWindowSize} />
        {windowSize === "custom" && <p className="ui-settings-note">{text("look-window-custom-note")}</p>}
      </Row>
    </Group>
    <Group id="look-text">
      <Row id="text-size" label={text("look-text-size")} {...feedback("text-size")}>
        <Segmented label={text("look-text-size")} value={p.larger_text ? "larger" : "default"}
          options={[{ value: "default", label: text("look-default-text") }, { value: "larger", label: text("look-larger-text") }]}
          onChange={(value) => change({ larger_text: value === "larger" })} />
      </Row>
    </Group>
  </>;
}
