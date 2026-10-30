// Invented content rendered by the production shared components. This entry is
// bundled in memory by readable-overlays.browser.mjs, without a fixture server.
import { useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { KEY_TABLE } from "../keys";
import { Button } from "./Button";
import { KeysDialog } from "./KeysSheet";
import { Menu, MenuItem, MenuSection } from "./Menu";
import { Option } from "./Option";
import { SettingsGroup, SettingsRow } from "./SettingsGroup";
import { Switch } from "./Switch";
import { ToolbarMenu } from "./ToolbarMenu";
import "../styles.css";

const longLabel = "An invented reference about readable interfaces and patient readers in a small window. ".repeat(3).slice(0, 200);
const description = "This invented Settings description stays readable when the available space is small and the text is enlarged. ".repeat(3);

function Fixture() {
  const field = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [checked, setChecked] = useState(false);
  const [value, setValue] = useState("format-0");
  return <>
    <header><input ref={field} aria-label="Search" />
      <ToolbarMenu label="Format" value={value} selectedKey={value} filterLabel="Filter formats"
        isOpen={open === "Format"} onOpenChange={(next) => setOpen(next ? "Format" : null)} returnFocusRef={field}
        onAction={setValue} sections={[{ id: "formats", label: "Citation formats", items: Array.from({ length: 20 }, (_, i) => ({ id: `format-${i}`, label: `Invented format ${i + 1}`, sample: "[@aster]" })) }]} />
      {["More", "Reference"].map((name) => <Menu key={name} label={name} trigger={name} isOpen={open === name}
        onOpenChange={(next) => setOpen(next ? name : null)} tabFocusRef={field}>
        <MenuSection id={`${name}-actions`} label="Actions">
          <MenuItem id="long" label={longLabel} keys="⇧⌘C" onAction={() => {}} />
          <MenuItem id="copy" label="Copy the invented reference" onAction={() => {}} />
        </MenuSection>
      </Menu>)}
      <Button onPress={() => setOpen("Keys")}>Keys</Button>
    </header>
    <div className="fixture-body">
      <ul role="listbox" aria-label="Invented results">
        {Array.from({ length: 12 }, (_, i) => <Option key={i} role="option" aria-selected={false}>
          <span className="result-body"><span className="result-line"><strong className="result-title">{longLabel}<span className="fixture-title-suffix"> · Invented study arm {i + 1}</span></strong></span>
            <span className="result-meta"><span className="result-byline"><span className="result-authors">Aster, Alder and Cypress</span><span className="result-separator"> · </span><span className="result-container">Invented Journal of Interfaces</span></span><span className="result-type">Article</span><span className="result-library">Invented collection</span></span></span><span className="result-year">2026</span>
        </Option>)}
      </ul>
      <SettingsGroup id="fixture-settings" title="Invented Settings">
        <SettingsRow id="fixture-row" label="Read the description" description={description}>
          <Switch aria-labelledby="fixture-row" isSelected={checked} onChange={setChecked} />
        </SettingsRow>
      </SettingsGroup>
    </div>
    {open === "Keys" && <KeysDialog bindings={KEY_TABLE} isMac onClose={() => setOpen(null)} />}
  </>;
}

createRoot(document.getElementById("fixture")!).render(<Fixture />);
