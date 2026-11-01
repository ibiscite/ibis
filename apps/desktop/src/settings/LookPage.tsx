import type { Accent, Appearance, PickerPreferences, WindowSize } from "../bindings";
import { useInterfaceMessages } from "../i18n";
import { PictureChoice } from "../ui/PictureChoice";
import { Segmented } from "../ui/Segmented";
import { SettingsGroup as Group, SettingsRow as Row } from "../ui/SettingsGroup";
import { Switch } from "../ui/Switch";
import { PickerPicture } from "./PickerPicture";

const ACCENTS: readonly Accent[] = ["system", "blue", "purple", "pink", "red", "orange", "yellow", "green", "graphite", "ibis"];
const APPEARANCES: readonly Appearance[] = ["system", "light", "dark"];
type Feedback = (id: string) => { highlighted: boolean; problem?: string };
const SIZES = { small: [64, 40], medium: [72, 48], large: [84, 60], custom: [72, 48] } as const;
function SizePicture({ size }: { size: WindowSize }) {
  const [width, height] = SIZES[size];
  return <span className="ui-size-picture" data-custom={size === "custom" || undefined} style={{ width, height }}>
    {size !== "custom" && <><b data-query /><b /><b data-short /></>}
  </span>;
}
export function LookPage({ preferences: p, change, feedback, windowSize, chooseWindowSize }: {
  preferences: PickerPreferences; change: (patch: Partial<PickerPreferences>) => void; feedback: Feedback;
  windowSize: WindowSize | null; chooseWindowSize: (size: WindowSize) => void;
}) {
  const { text } = useInterfaceMessages();
  const appearance = p.appearance ?? "system", accent = p.accent ?? "system";
  const sample = { density: p.density, icon: p.search_icon };
  return <>
    <Group id="look-appearance" title={text("look-appearance")}>
      <Row bare id="appearance" label={text("look-appearance")} {...feedback("appearance")}>
        <PictureChoice label={text("look-appearance")} value={appearance}
          options={APPEARANCES.map((value) => ({ value, label: text(`look-${value}`), picture: <PickerPicture {...sample} appearance={value === "system" ? "light" : value} split={value === "system"} /> }))}
          onChange={(appearance) => change({ appearance })} />
      </Row>
    </Group>
    <Group id="look-accent">
      <Row id="accent" label={text("look-accent")} description={accent === "system" ? text("look-accent-multicolor") : text("look-accent-inside", { name: text(`look-accent-${accent}`) })} {...feedback("accent")}>
        <PictureChoice dots label={text("look-accent")} description="accent-description" value={accent}
          options={ACCENTS.map((value) => ({ value, label: text(`look-accent-${value}`), picture: <span className="ui-accent-dot" data-accent={value} /> }))}
          onChange={(accent) => change({ accent })} />
      </Row>
    </Group>
    <Group id="look-rows" title={text("general-rows")}>
      <Row bare id="rows" label={text("general-rows")} {...feedback("rows")}>
        <PictureChoice strips label={text("general-rows")} value={p.density}
          options={(["compact", "comfortable", "spacious"] as const).map((value) => ({ value, label: text(`general-density-${value}`), note: text(`look-density-${value}`), picture: <PickerPicture rowsOnly density={value} icon={p.search_icon} /> }))}
          onChange={(density) => change({ density })} />
      </Row>
    </Group>
    <Group id="look-size" title={text("look-window-size")}>
      <Row bare id="window-size" label={text("look-window-size")} {...feedback("window-size")}>
        <PictureChoice label={text("look-window-size")} value={windowSize ?? "medium"} isDisabled={windowSize === null}
          options={(windowSize === "custom" ? ["small", "medium", "large", "custom"] as const : ["small", "medium", "large"] as const)
            .map((value) => ({ value, label: text(`look-window-${value}`), picture: <SizePicture size={value} /> }))}
          onChange={chooseWindowSize} />
        {windowSize === "custom" && <p className="ui-settings-note">{text("look-window-custom-note")}</p>}
      </Row>
    </Group>
    <Group id="look-icon" title={text("look-search-icon")}>
      <Row bare id="search-icon" label={text("look-search-icon")} {...feedback("search-icon")}>
        <PictureChoice label={text("look-search-icon")} value={p.search_icon}
          options={([
            { value: "ibis", label: text("look-ibis"), picture: <PickerPicture headerOnly icon="ibis" /> },
            { value: "magnifier", label: text("look-magnifier"), picture: <PickerPicture headerOnly icon="magnifier" /> },
            { value: "none", label: text("look-none"), picture: <PickerPicture headerOnly icon="none" /> },
          ] as const)} onChange={(search_icon) => change({ search_icon })} />
      </Row>
    </Group>
    <Group id="look-text">
      <Row id="text-size" label={text("look-text-size")} {...feedback("text-size")}>
        <Segmented label={text("look-text-size")} value={p.larger_text ? "larger" : "default"}
          options={[{ value: "default", label: text("look-default-text") }, { value: "larger", label: text("look-larger-text") }]}
          onChange={(value) => change({ larger_text: value === "larger" })} />
      </Row>
      <Row id="preview" label={text("general-preview")} description={text("general-preview-description")} {...feedback("preview")}>
        <Switch aria-labelledby="preview" aria-describedby="preview-description" isSelected={p.show_paste_preview} onChange={(show_paste_preview) => change({ show_paste_preview })} />
      </Row>
    </Group>
  </>;
}
