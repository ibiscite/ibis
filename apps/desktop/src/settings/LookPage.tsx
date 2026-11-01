import type { Accent, Appearance, PickerPreferences, WindowSize } from "../bindings";
import { useInterfaceMessages } from "../i18n";
import { PictureChoice } from "../ui/PictureChoice";
import { Segmented } from "../ui/Segmented";
import { SettingsGroup as Group, SettingsRow as Row } from "../ui/SettingsGroup";
import { Switch } from "../ui/Switch";
import { PickerPicture } from "./PickerPicture";
import { rowDetails } from "./rows";

const ACCENTS: readonly Accent[] = ["system", "ibis", "blue", "purple", "pink", "red", "orange", "yellow", "green", "graphite"];
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
  return <Group id="look-choices">
    <Row id="appearance" {...rowDetails("appearance", text)} {...feedback("appearance")}>
      <PictureChoice label={text("look-appearance")} description="appearance-description" value={appearance}
        options={APPEARANCES.map((value) => ({ value, label: text(`look-${value}`), picture: <PickerPicture density={p.density} icon={p.search_icon} appearance={value === "system" ? "light" : value} split={value === "system"} /> }))}
        onChange={(appearance) => change({ appearance })} />
    </Row>
    <Row id="accent" {...rowDetails("accent", text)} {...feedback("accent")}>
      <PictureChoice dots label={text("look-accent")} description="accent-description" value={accent}
        options={ACCENTS.map((value) => ({ value, label: text(`look-accent-${value}`), showLabel: value === "ibis", picture: <span className="ui-accent-dot" data-accent={value} /> }))}
        onChange={(accent) => change({ accent })} />
    </Row>
    <Row id="rows" label={text("general-rows")} {...feedback("rows")}>
      <PictureChoice label={text("general-rows")} value={p.density}
        options={(["compact", "comfortable", "spacious"] as const).map((value) => ({ value, label: text(`general-density-${value}`), picture: <PickerPicture rowsOnly density={value} icon={p.search_icon} /> }))}
        onChange={(density) => change({ density })} />
    </Row>
    <Row id="window-size" label={text("look-window-size")} description={windowSize === "custom" ? text("look-window-custom-note") : undefined} {...feedback("window-size")}>
      <PictureChoice label={text("look-window-size")} value={windowSize ?? "medium"} isDisabled={windowSize === null}
        options={(windowSize === "custom" ? ["small", "medium", "large", "custom"] as const : ["small", "medium", "large"] as const)
          .map((value) => ({ value, label: text(`look-window-${value}`), picture: <SizePicture size={value} /> }))}
        onChange={chooseWindowSize} />
    </Row>
    <Row id="search-icon" label={text("look-search-icon")} {...feedback("search-icon")}>
      <PictureChoice label={text("look-search-icon")} value={p.search_icon}
        options={([
          { value: "ibis", label: text("look-ibis"), picture: <PickerPicture headerOnly icon="ibis" /> },
          { value: "magnifier", label: text("look-magnifier"), picture: <PickerPicture headerOnly icon="magnifier" /> },
          { value: "none", label: text("look-none"), picture: <PickerPicture headerOnly icon="none" /> },
        ] as const)} onChange={(search_icon) => change({ search_icon })} />
    </Row>
    <Row id="text-size" label={text("look-text-size")} {...feedback("text-size")}>
      <Segmented label={text("look-text-size")} value={p.larger_text ? "larger" : "default"}
        options={[{ value: "default", label: text("look-default-text") }, { value: "larger", label: text("look-larger-text") }]}
        onChange={(value) => change({ larger_text: value === "larger" })} />
    </Row>
    <Row id="preview" label={text("general-preview")} description={text("general-preview-description")} {...feedback("preview")}>
      <Switch aria-labelledby="preview" aria-describedby="preview-description" isSelected={p.show_paste_preview} onChange={(show_paste_preview) => change({ show_paste_preview })} />
    </Row>
  </Group>;
}
