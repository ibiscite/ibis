import type { Accent, Appearance, PickerPreferences } from "../bindings";
import { useInterfaceMessages } from "../i18n";
import { PictureChoice } from "../ui/PictureChoice";
import { Segmented } from "../ui/Segmented";
import { SettingsGroup as Group, SettingsRow as Row } from "../ui/SettingsGroup";
import { Switch } from "../ui/Switch";
import { PickerPicture } from "./PickerPicture";

const ACCENTS: readonly Accent[] = ["system", "blue", "purple", "pink", "red", "orange", "yellow", "green", "graphite"];
const APPEARANCES: readonly Appearance[] = ["system", "light", "dark"];
type Feedback = (id: string) => { highlighted: boolean; problem?: string };
export function LookPage({ preferences: p, change, feedback }: {
  preferences: PickerPreferences; change: (patch: Partial<PickerPreferences>) => void; feedback: Feedback;
}) {
  const { text } = useInterfaceMessages();
  const appearance = p.appearance ?? "system", accent = p.accent ?? "system";
  return <Group id="look-choices">
    <Row id="appearance" label={text("look-appearance")} description={text("look-appearance-description")} {...feedback("appearance")}>
      <PictureChoice label={text("look-appearance")} description="appearance-description" value={appearance}
        options={APPEARANCES.map((value) => ({ value, label: text(`look-${value}`), picture: <PickerPicture density={p.density} icon={p.search_mark ? "ibis" : "magnifier"} appearance={value === "system" ? "light" : value} split={value === "system"} /> }))}
        onChange={(appearance) => change({ appearance })} />
    </Row>
    <Row id="accent" label={text("look-accent")} description={text("look-accent-description")} {...feedback("accent")}>
      <PictureChoice dots label={text("look-accent")} description="accent-description" value={accent}
        options={ACCENTS.map((value) => ({ value, label: text(`look-accent-${value}`), picture: <span className="ui-accent-dot" data-accent={value} /> }))}
        onChange={(accent) => change({ accent })} />
    </Row>
    <Row id="rows" label={text("general-rows")} {...feedback("rows")}>
      <PictureChoice label={text("general-rows")} value={p.density}
        options={(["compact", "comfortable", "spacious"] as const).map((value) => ({ value, label: text(`general-density-${value}`), picture: <PickerPicture density={value} icon={p.search_mark ? "ibis" : "magnifier"} /> }))}
        onChange={(density) => change({ density })} />
    </Row>
    <Row id="window-size" label={text("look-window-size")} description={text("settings-coming-soon")} {...feedback("window-size")}>
      <PictureChoice label={text("look-window-size")} description="window-size-description" value="" isDisabled
        options={(["small", "medium", "large", "compact-window", "custom"] as const).map((value) => ({ value, label: text(`look-${value}`) }))} onChange={() => undefined} />
    </Row>
    <Row id="search-icon" label={text("look-search-icon")} {...feedback("search-icon")}>
      <PictureChoice label={text("look-search-icon")} value={p.search_mark ? "ibis" : "magnifier"}
        options={([
          { value: "ibis", label: text("look-ibis"), picture: <PickerPicture headerOnly icon="ibis" /> },
          { value: "magnifier", label: text("look-magnifier"), picture: <PickerPicture headerOnly icon="magnifier" /> },
          { value: "none", label: text("look-none"), isDisabled: true },
        ] as const)} onChange={(value) => change({ search_mark: value === "ibis" })} />
    </Row>
    <Row id="text-size" label={text("look-text-size")} {...feedback("text-size")}>
      <Segmented label={text("look-text-size")} value={p.larger_text ? "larger" : "default"}
        options={[{ value: "default", label: text("look-default-text") }, { value: "larger", label: text("look-larger-text") }, { value: "largest", label: text("look-largest-text"), isDisabled: true }]}
        onChange={(value) => change({ larger_text: value === "larger" })} />
    </Row>
    <Row id="preview" label={text("general-preview")} description={text("general-preview-description")} {...feedback("preview")}>
      <Switch aria-labelledby="preview" aria-describedby="preview-description" isSelected={p.show_paste_preview} onChange={(show_paste_preview) => change({ show_paste_preview })} />
    </Row>
  </Group>;
}
