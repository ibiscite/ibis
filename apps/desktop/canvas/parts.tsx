import { useRef, type ReactNode } from "react";
import { ChoiceIcon } from "../src/icons";
import { Button } from "../src/ui/Button";
import { IconButton } from "../src/ui/IconButton";
import { StatusMark } from "../src/ui/StatusMark";
import { Icon } from "../src/icons";
import { iconPaths } from "../src/icon-paths";
import { ChipGroup } from "../src/ui/ChipGroup";
import { ChoiceList } from "../src/ui/ChoiceList";
import { IbisMark } from "../src/ui/IbisMark";
import { KeyCaps, KeyHint } from "../src/ui/KeyCaps";
import { PictureChoice } from "../src/ui/PictureChoice";
import { Segmented } from "../src/ui/Segmented";
import { SettingsGroup, SettingsRow } from "../src/ui/SettingsGroup";
import { SourceAction } from "../src/ui/SourceAction";
import { Switch } from "../src/ui/Switch";

const ignore = () => {};
const states = ["rest", "hovered", "pressed", "focus-visible", "disabled"] as const;
const variants = ["primary", "secondary"] as const;
const dots = [["#007aff", "Blue"], ["#953d96", "Purple"], ["#e0383e", "Red"], ["#62ba46", "Green"]] as const;
const choices = [
  { id: "retry", title: "Try again", help: "Read the library again", icon: <ChoiceIcon name="retry" />, suggested: true },
  { id: "another", title: "Use another library", help: "Zotero or another library file", icon: <ChoiceIcon name="swap" /> },
];

function State({ name, children }: { name: string; children: ReactNode }) {
  return <div className="canvas-part"><div className="canvas-part-items">{children}</div><code>{name}</code></div>;
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return <section><h2>{title}</h2><div className="canvas-parts-row">{children}</div></section>;
}

/** Every shared part, drawn by its own component. A new part belongs here. */
export function Parts() {
  const fallback = useRef<HTMLDivElement>(null);
  return <div className="canvas-parts" ref={fallback}>
    {variants.map((variant) => <Section key={variant} title={`Button: ${variant}`}>
      {states.map((state) => <State key={state} name={state}>
        <Button variant={variant} isDisabled={state === "disabled"} render={(native) => <button {...native} {...{ [`data-${state}`]: state === "rest" ? undefined : true }} />}>Copy as Pandoc</Button>
      </State>)}
    </Section>)}
    {(["regular", "large"] as const).map((size) => <Section key={size} title={`IconButton: ${size}`}>
      {(["rest", "hovered", "pressed", "selected", "focus-visible", "disabled"] as const).map((state) => <State key={state} name={state === "selected" ? "toggled-on" : state}>
        <IconButton icon="pin" size={size} aria-label="Keep Window Open" isSelected={state === "selected"} isDisabled={state === "disabled"}
          render={(native) => <button {...native} {...{ [`data-${state}`]: state === "rest" ? undefined : true }} />} />
      </State>)}
    </Section>)}
    <Section title="Status: dot, symbol, wash">
      {(["ok", "warning", "error"] as const).map((kind) => <State key={kind} name={kind}>
        <StatusMark kind={kind} presentation="dot" /><StatusMark kind={kind} /><StatusMark kind={kind} presentation="wash" />
      </State>)}
    </Section>
    <Section title="Switch and segmented control">
      <State name="off"><Switch aria-label="Off" isSelected={false} onChange={ignore} /></State>
      <State name="on"><Switch aria-label="On" isSelected onChange={ignore} /></State>
      <State name="disabled"><Switch aria-label="Disabled" isSelected isDisabled onChange={ignore} /></State>
      <State name="segmented"><Segmented label="Text size" value="default" onChange={ignore} options={[{ value: "default", label: "Default" }, { value: "larger", label: "Larger" }]} /></State>
    </Section>
    <Section title="Keys">
      <State name="one key"><KeyCaps accelerator="Enter" /></State>
      <State name="chord"><KeyCaps accelerator="CmdOrCtrl+Shift+P" /></State>
      <State name="hint"><KeyHint label="More" keys="CmdOrCtrl+K" /></State>
    </Section>
    <Section title="Chips and source action">
      <State name="chips"><ChipGroup label="Libraries" items={[{ id: "a", label: "invented.bib", textValue: "invented.bib", removeLabel: "Remove invented.bib" }, { id: "b", label: "Zotero", textValue: "Zotero", removeLabel: "Remove Zotero" }]} onRemove={ignore} fallbackFocusRef={fallback} /></State>
      <State name="source"><SourceAction label="Open invented.bib" keys="CmdOrCtrl+O" icon={null} file open={ignore} /></State>
    </Section>
    <Section title="Pictures and dots">
      <State name="accent"><PictureChoice dots label="Accent" value="Blue" onChange={ignore} options={dots.map(([colour, label]) => ({ value: label, label, picture: <span className="ui-accent-dot" style={{ background: colour }} /> }))} /></State>
    </Section>
    <section><h2>Settings rows</h2>
      <SettingsGroup id="canvas-group" title="Group title">
        <SettingsRow id="canvas-switch" label="A setting" description="One line that explains it"><Switch aria-label="A setting" isSelected onChange={ignore} /></SettingsRow>
        <SettingsRow id="canvas-button" label="An action"><Button>Check now</Button></SettingsRow>
        <SettingsRow id="canvas-highlight" label="Found by search" highlighted><Button>Reset</Button></SettingsRow>
      </SettingsGroup>
    </section>
    <section><h2>Choice rows</h2><ChoiceList title="A choice screen" line="One line of context." choices={choices} onChoose={ignore} /></section>
    <Section title="Marks and icons">
      <State name="mark"><IbisMark size="welcome" className="canvas-part-mark" /></State>
      {(Object.keys(iconPaths) as (keyof typeof iconPaths)[]).map((name) => <State key={name} name={name}><Icon name={name} /></State>)}
    </Section>
  </div>;
}
