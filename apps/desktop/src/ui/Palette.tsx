import { useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type ReactNode, type RefObject } from "react";
import { flushSync } from "react-dom";
import { composing } from "../composition";
import { Icon } from "../icons";
import { ariaKeys, keyLabel } from "../keys";
import { currentPlatform } from "../platform";
import { Button } from "./Button";
import { ViewFrame, leaveView } from "./ViewFrame";
import { say } from "../i18n";
import { useLayer } from "./layers";
import "./Palette.css";

/** One row: a name, a quiet detail only when it tells rows apart, its keys, and
 * a check on the current choice. `back` rows undo a remembered choice (↩). A
 * disabled row stays in the list with its reason as its detail. */
export type PaletteItem = {
  id: string; label: string; detail?: string; keywords?: string; keys?: string;
  icon?: ReactNode; checked?: boolean; disabled?: boolean; back?: boolean;
};
/** A titled group. `browsing` groups show only before typing (your few formats);
 * `searching` ones only while typing (the whole catalogue). */
export type PaletteSection = { id: string; label?: string; /** The group's spoken name, when it says more than the heading. */ name?: string; items: readonly PaletteItem[]; only?: "browsing" | "searching" };
export type PaletteClose = "action" | "dismiss" | "command" | "tab";

const fold = (text: string) => text.normalize("NFD").replace(/\p{M}/gu, "").toLocaleLowerCase();

/** What a query leaves: every word must appear in a row's name, detail or synonyms. */
export function filterPalette(sections: readonly PaletteSection[], query: string): PaletteSection[] {
  const words = fold(query).split(/\s+/).filter(Boolean);
  return sections.filter((section) => (words.length ? section.only !== "browsing" : section.only !== "searching"))
    .map((section) => ({ ...section, items: section.items.filter((item) => {
      const text = fold([item.label, item.detail, item.keywords].filter(Boolean).join(" "));
      return words.every((word) => text.includes(word));
    }) }))
    .filter((section) => section.items.length > 0);
}

/** The palette: one searchable view of the window for every choice that isn't
 * search itself (the format, every action), as Raycast's action panel. It
 * takes the picker's place (ui/ViewFrame), takes keys from its first frame, and Esc, its own
 * key or a pick closes it. ↑ ↓ move, Enter runs the highlighted row. An
 * optional preview shows what the highlighted row would do. */
export function Palette({ id, title, subtitle, description, placeholder, sections, onAction, onClose, preview, actionLabel, empty, initial, invoker }: {
  id: string;
  /** The palette's name, also the token before the field ("Format", "Actions"). */
  title: string;
  /** Whose choice this is ("For Word", "Default format"), in the footer's status place. */
  subtitle?: string | null;
  /** Why the choice stands as it does, read with the palette and shown under the preview. */
  description?: string;
  placeholder: string;
  sections: readonly PaletteSection[];
  onAction: (id: string) => void;
  /** Closed. On a dismissal the palette has already returned focus to its opener; the caller may move it on. */
  onClose: (reason: PaletteClose) => void;
  /** What the highlighted row would do, shown beside the list. */
  preview?: (item: PaletteItem) => ReactNode;
  /** The footer's words for Enter on the highlighted row ("Use for Word"). */
  actionLabel: (item: PaletteItem) => string;
  /** Said when nothing matches. */
  empty: string;
  /** The row highlighted on opening, such as the current format. */
  initial?: string;
  /** The button that opens it. Esc and Back return the keyboard there, as a menu
   * returns to its button, however the palette was opened (its key or a click). */
  invoker?: RefObject<HTMLElement | null>;
}) {
  const [query, setQuery] = useState("");
  const shown = useMemo(() => filterPalette(sections, query), [sections, query]);
  const items = useMemo(() => shown.flatMap((section) => section.items), [shown]);
  const [highlight, setHighlight] = useState<string | undefined>(initial);
  const current = items.find((item) => item.id === highlight) ?? items[0];
  const listId = useId(), titleId = useId(), descriptionId = useId();
  const optionId = (item: PaletteItem) => `${id}-${item.id}`;
  const field = useRef<HTMLInputElement>(null), frame = useRef<HTMLDivElement>(null);
  const leave = () => { if (frame.current) leaveView(frame.current); };
  const mac = currentPlatform() === "macos";
  // Esc, a click outside or Back hands the keyboard back to whatever had it.
  const opener = useRef<Element | null>(null);
  // Once a row runs, the palette is done: an action clearing the way (a sheet
  // opening) closes it without handing focus back.
  const acted = useRef(false);
  const dismiss = () => {
    if (acted.current) return;
    leave();
    const back = invoker?.current ?? opener.current;
    if (back instanceof HTMLElement && back.isConnected && !back.closest("[hidden], [inert]")) back.focus({ preventScroll: true });
    onClose("dismiss");
  };
  // A command clearing the way needs the palette gone before it acts.
  useLayer("menu", true, () => { flushSync(dismiss); });
  useLayoutEffect(() => { opener.current = document.activeElement; field.current?.focus({ preventScroll: true }); }, []);
  useLayoutEffect(() => {
    if (current) document.getElementById(optionId(current))?.scrollIntoView?.({ block: "nearest" });
  });
  const run = (item: PaletteItem | undefined) => {
    if (!item || item.disabled || acted.current) return;
    acted.current = true;
    leave();
    onAction(item.id);
  };
  const move = (step: number) => {
    if (!items.length) return;
    const at = current ? items.indexOf(current) : -1;
    setHighlight(items[Math.min(Math.max(at + step, 0), items.length - 1)]!.id);
  };
  // Keys belong to the whole palette, so they work wherever in it the keyboard is.
  const keyDown = (event: ReactKeyboardEvent) => {
    if (composing(event.nativeEvent)) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") { event.preventDefault(); move(event.key === "ArrowDown" ? 1 : -1); }
    else if (event.key === "Home" && !query) { event.preventDefault(); move(-items.length); }
    else if (event.key === "End" && !query) { event.preventDefault(); move(items.length); }
    else if (event.key === "Enter") { event.preventDefault(); if (!event.repeat) run(current); }
    // Tab leaves the palette for Search, as Tab leaves a menu.
    else if (event.key === "Tab") { event.preventDefault(); acted.current = true; leave(); onClose("tab"); }
  };
  return <ViewFrame title={title} titleId={titleId} role="dialog" aria-modal="true" aria-describedby={description ? descriptionId : undefined}
    back={{ label: say("palette-back"), onBack: dismiss }} status={subtitle} frameRef={frame}
    onKeyDown={keyDown}
    // A press anywhere but the field leaves the keyboard in the field; clicks still act.
    onMouseDown={(event) => { if (event.target !== field.current) event.preventDefault(); }}
    search={<input ref={field} className="ui-view-field" type="text" role="combobox" aria-label={placeholder} placeholder={placeholder}
      aria-expanded="true" aria-controls={listId} aria-autocomplete="list" aria-activedescendant={current ? optionId(current) : undefined}
      autoComplete="off" spellCheck={false} value={query}
      onChange={(event) => { setQuery(event.target.value); setHighlight(undefined); }} />}
    actions={current && !current.disabled ? <Button variant="action" size="small" className="ui-palette-action" excludeFromTabOrder onPress={() => run(current)}
      label={actionLabel(current)} keys="Enter" /> : undefined}>
    <div className="ui-palette" data-preview={preview ? "" : undefined}>
      <ul id={listId} role="listbox" aria-label={title} className="ui-palette-list scroll-area">
        {shown.map((section) => <li key={section.id} role="presentation" className="ui-palette-section">
          {section.label && <div className="ui-palette-heading" aria-hidden="true">{section.label}</div>}
          <ul role="group" aria-label={section.name ?? section.label ?? title}>
            {section.items.map((item) => <li key={item.id} id={optionId(item)} role="option" className="ui-palette-row"
              aria-selected={item === current} aria-current={item.checked || undefined} aria-disabled={item.disabled || undefined}
              aria-keyshortcuts={item.keys ? ariaKeys(item.keys, mac) : undefined} data-active={item === current || undefined}
              onPointerMove={() => { if (highlight !== item.id) setHighlight(item.id); }}
              onClick={() => run(item)}>
              <span className="ui-palette-lead" aria-hidden="true">{item.back ? <Icon name="back" /> : item.checked ? <Icon name="check" /> : item.icon}</span>
              <span className="ui-palette-label">{item.label}</span>
              {item.detail && <span className="ui-palette-detail">{item.detail}</span>}
              {item.keys && <span className="ui-palette-keys" aria-hidden="true">{keyLabel(item.keys, mac)}</span>}
            </li>)}
          </ul>
        </li>)}
        {!items.length && <li role="presentation" className="ui-palette-empty">{empty}</li>}
      </ul>
      {preview && <div className="ui-palette-preview">
        <div aria-live="polite">{current && preview(current)}</div>
        {description && <p id={descriptionId} className="ui-palette-preview-note">{description}</p>}
      </div>}
      {!preview && description && <p id={descriptionId} className="visually-hidden">{description}</p>}
    </div>
  </ViewFrame>;
}
