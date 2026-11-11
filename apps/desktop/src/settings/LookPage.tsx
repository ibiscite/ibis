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

/** Appearance (maintainer's pick, 2026-11-10, option C): one live picture of the
 * picker answers "what will this look like?" for every choice under it, and each
 * choice is its own plain control, so the page reads as one group. */
export function LookPage({ preferences: p, change, feedback, windowSize, chooseWindowSize }: {
  preferences: PickerPreferences; change: (patch: Partial<PickerPreferences>) => void; feedback: Feedback;
  windowSize: WindowSize | null; chooseWindowSize: (size: WindowSize) => void;
}) {
  const platform = currentPlatform();
  const { text } = useInterfaceMessages();
  const appearance = p.appearance ?? "system", accent = p.accent ?? "system";
  const size = windowSize ?? "medium";
  return <Group id="look-appearance">
    <div className="ui-picture-preview" aria-hidden="true">
      <PickerPicture density={p.density} appearance={appearance === "system" ? "light" : appearance} split={appearance === "system"} frame={WINDOWS[size]} />
    </div>
    <Row id="appearance" label={text("look-appearance")} {...feedback("appearance")}>
      <PictureChoice size="small" label={text("look-appearance")} value={appearance}
        options={APPEARANCES.map((value) => ({ value, label: text(`look-${value}`), picture: <PickerPicture density={p.density} appearance={value === "system" ? "light" : value} split={value === "system"} /> }))}
        onChange={(appearance) => change({ appearance })} />
    </Row>
    <Row id="accent" label={text("look-accent")} {...feedback("accent")}>
      <Segmented label={text("look-accent")} value={accent}
        options={ACCENTS.map((value) => ({ value, label: text(`look-accent-${value}`, { platform }) }))}
        onChange={(accent) => change({ accent })} />
    </Row>
    <Row id="rows" label={text("general-rows")} {...feedback("rows")}>
      <Segmented label={text("general-rows")} value={p.density}
        options={(["compact", "comfortable"] as const).map((value) => ({ value, label: text(`general-density-${value}`) }))}
        onChange={(density) => change({ density })} />
    </Row>
    <Row id="window-size" label={text("look-window-size")} {...feedback("window-size")}>
      <Segmented label={text("look-window-size")} value={size}
        options={(windowSize === "custom" ? ["small", "medium", "large", "custom"] as const : ["small", "medium", "large"] as const)
          .map((value) => ({ value, label: text(`look-window-${value}`), isDisabled: windowSize === null }))}
        onChange={chooseWindowSize} />
      {windowSize === "custom" && <p className="ui-settings-note">{text("look-window-custom-note")}</p>}
    </Row>
    <Row id="text-size" label={text("look-text-size")} {...feedback("text-size")}>
      <Segmented label={text("look-text-size")} value={p.larger_text ? "larger" : "default"}
        options={[{ value: "default", label: text("look-default-text") }, { value: "larger", label: text("look-larger-text") }]}
        onChange={(value) => change({ larger_text: value === "larger" })} />
    </Row>
  </Group>;
}
