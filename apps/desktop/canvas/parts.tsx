import { useLayoutEffect, useRef, type ReactNode } from "react";
import { ChoiceIcon, PageIcon } from "../src/icons";
import { Button } from "../src/ui/Button";
import { IconButton } from "../src/ui/IconButton";
import { StatusMark } from "../src/ui/StatusMark";
import { Icon } from "../src/icons";
import { iconPaths } from "../src/icon-paths";
import { ChipGroup } from "../src/ui/ChipGroup";
import { ChoiceList } from "../src/ui/ChoiceList";
import { IbisMark } from "../src/ui/IbisMark";
import { KeyCaps, KeyHint } from "../src/ui/KeyCaps";
import { NoticeSlot, QuietNotice } from "../src/ui/NoticeSlot";
import { Segmented } from "../src/ui/Segmented";
import { Group, Row, Section as LayoutSection } from "../src/ui/Layout";
import { ShelfSegment } from "../src/ui/ShelfSegment";
import { ToolbarMenu } from "../src/ui/ToolbarMenu";
import { SideResults } from "../src/ui/SideNavigation";
import { SourceAction } from "../src/ui/SourceAction";
import { Switch } from "../src/ui/Switch";
import { UndoNotice } from "../src/ui/UndoNotice";

const ignore = () => {};
const states = ["rest", "hovered", "pressed", "focus-visible", "disabled"] as const;
const variants = ["primary", "secondary"] as const;
const choices = [
  { id: "retry", title: "Try Again", help: "Read the library again", icon: <ChoiceIcon name="retry" />, suggested: true },
  { id: "another", title: "Use Another Library", help: "Zotero or another library file", icon: <ChoiceIcon name="swap" /> },
];

function State({ name, children }: { name: string; children: ReactNode }) {
  return <div className="canvas-part"><div className="canvas-part-items">{children}</div><code>{name}</code></div>;
}

/** The shelf band around one of its parts, as the picker draws it. `hovered` marks the
 * pointed state on the element named, as React Aria marks a hovered button. */
function Band({ hovered, children }: { hovered?: string; children: ReactNode }) {
  const band = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => { if (hovered) band.current?.querySelector(hovered)?.setAttribute("data-hovered", "true"); }, [hovered]);
  return <div ref={band} className="shelf canvas-band"><div className="shelf-output"><div className="shelf-citation">{children}</div></div></div>;
}
const segment = (state: "rest" | "hovered" | "expanded" | "focus-visible") => <ShelfSegment reference={state} referenceKey="vale2021" aria-label="Edit Vale 2021"
  aria-expanded={state === "expanded"} remove={{ label: "Remove Vale 2021", onPress: ignore }}
  render={state === "focus-visible" ? (native) => <button {...native} data-focus-visible /> : undefined}><bdi className="preview-text">(Vale 2021)</bdi></ShelfSegment>;
const count = <ToolbarMenu label="References in the citation" value="2" selectedKey="" onAction={ignore}
  button={{ className: "shelf-count", quiet: true, label: "2 references", content: "2 references" }}
  sections={[{ id: "references", items: [{ id: "a", label: "Vale 2021" }, { id: "b", label: "Reed 2019" }] }]} />;

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
    <Section title="Shelf: segments and the reference count">
      {(["rest", "hovered", "expanded", "focus-visible"] as const).map((state) => <State key={state} name={state}>
        <Band hovered={state === "hovered" ? ".shelf-segment" : undefined}>{segment(state)}</Band>
      </State>)}
      <State name="count"><Band>{count}</Band></State>
      <State name="count hovered"><Band hovered=".shelf-count">{count}</Band></State>
    </Section>
    <section><h2>Settings rows</h2>
      <LayoutSection id="canvas-group" title="Group title"><Group>
        <Row id="canvas-switch" label="A setting" description="One line that explains it"><Switch aria-label="A setting" isSelected onChange={ignore} /></Row>
        <Row id="canvas-button" label="An action"><Button>Check Now</Button></Row>
        <Row id="canvas-highlight" label="Found by search" highlighted><Button>Reset</Button></Row>
      </Group></LayoutSection>
    </section>
    <section><h2>Choice rows</h2><ChoiceList title="A choice screen" line="One line of context." choices={choices} onChoose={ignore} /></section>
    <section><h2>Notices</h2>
      <NoticeSlot><UndoNotice message="Removed Vale 2021" action="Remove Vale 2021" onUndo={async () => {}} onExpire={ignore} /></NoticeSlot>
      <NoticeSlot><QuietNotice passive>1,000 references. A solid library.</QuietNotice></NoticeSlot>
    </section>
    <section><h2>Search results</h2><SideResults label="Search results" status="2 results" empty="No matching settings" onChoose={ignore}
      results={[{ id: "appearance", label: "Appearance", page: "look", pageLabel: "Look", tile: <PageIcon name="look" /> }, { id: "forget", label: "A setting whose name runs on to a second line in the sidebar", page: "citations", pageLabel: "Citations", tile: <PageIcon name="citations" /> }]} /></section>
    <Section title="Marks and icons">
      <State name="mark"><IbisMark size="welcome" className="canvas-part-mark" /></State>
      {(Object.keys(iconPaths) as (keyof typeof iconPaths)[]).map((name) => <State key={name} name={name}><Icon name={name} /></State>)}
    </Section>
  </div>;
}
