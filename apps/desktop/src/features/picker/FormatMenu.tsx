import type { RefObject } from "react";
import { ariaKeys, keysFor } from "../../keys";
import { isMac } from "../../platform";
import { COPY_FORMATS, type AppPaste } from "../../api";
import { FORMATTED_STYLES } from "../../format/styles";
import { STYLE_SAMPLES } from "../../format/styles";
import { copyFormatMenu, pickerText, writingProjectText } from "../../messages";
import { ToolbarMenu, type ToolbarMenuSection, type MenuCloseReason } from "../../ui/ToolbarMenu";
import { outputName, type Choice } from "./choice";
import type { FormatPick, FormatSource } from "./useDestination";

/** The copy format menu: key syntaxes and citation styles, each with a real
 * sample. Picking one remembers it for the app; without an app it is the
 * default. The menu says which applies and why.
 * Styles stay listed but unavailable while the engine is not offered.
 * Focus goes back to the search box after a pick, Escape or Tab. */
export function FormatMenu({ format, buttonRef, app, source, pick, onOpenChange, pasteChoices, onPasteChange, disabled, styles, onChange, searchRef, writingProject, onWritingProjects }: {
  format: Choice;
  buttonRef?: RefObject<HTMLButtonElement | null>;
  /** The app the picker was opened from, named beside the format. */
  app: string | null;
  /** Why the next paste uses `format`. */
  source: FormatSource;
  /** Where a pick goes. */
  pick: FormatPick;
  onOpenChange?: (open: boolean, reason?: MenuCloseReason) => void;
  pasteChoices?: boolean;
  onPasteChange?: (paste: AppPaste) => void;
  disabled: boolean;
  /** Whether formatted citation styles can be used now. */
  styles: boolean;
  onChange: (choice: Choice) => void;
  searchRef: RefObject<HTMLElement | null>;
  /** Explicit project context, distinct from the destination app. */
  writingProject?: { name: string } | null;
  onWritingProjects?: () => void;
}) {
  const sections: ToolbarMenuSection[] = [
    ...onWritingProjects ? [{ id: "project-actions", items: [{ id: "writing-projects", label: writingProjectText.menu, command: true }] }] : [],
    ...pasteChoices && app ? [{ id: "delivery", label: pickerText.forApp(app), items: [
      { id: "paste", label: pickerText.pasteFromNowOn(app), command: true },
      { id: "copy-only", label: pickerText.copyOnly, command: true },
    ] }] : [],
    { id: "key-formats", label: pickerText.keyFormats, items: COPY_FORMATS.map(({ value }) => ({ id: value, ...copyFormatMenu[value] })) },
    { id: "citation-styles", label: pickerText.formattedStyles, items: FORMATTED_STYLES.map(({ id }) => (
      { id: `style:${id}`, label: outputName(`style:${id}`), sample: STYLE_SAMPLES[id], prose: true, disabled: !styles })) },
  ];
  const name = outputName(format);
  const value = writingProject ? pickerText.formatForApp(name, writingProject.name) : source === "held" ? pickerText.formatThisPaste(name) : source === "default" || !app ? pickerText.formatDefault(name) : pickerText.formatForApp(name, app);
  const why = writingProject ? writingProjectText.remembered(writingProject.name) : source === "held" ? pickerText.formatHeld(name) : source === "default" ? app ? pickerText.formatAppDefault(app) : pickerText.formatNoApp
    : app ? source === "remembered" ? pickerText.formatRemembered(app) : pickerText.formatSuggested(app) : pickerText.formatNoApp;
  const next = writingProject ? writingProjectText.pick : source === "held" ? pick === "app" ? pickerText.pickToRemember : pick === "default" ? pickerText.pickToSetDefault : ""
    : pick === "paste" ? pickerText.pickThisPaste : pick === "default" ? pickerText.pickDefault : source === "remembered" ? pickerText.pickReplaces : pickerText.pickRemembered;
  return <><span role="status" className="visually-hidden">{`${name}. ${why}`}</span><ToolbarMenu buttonRef={buttonRef} onOpenChange={onOpenChange} buttonShortcut={ariaKeys(keysFor("format"), isMac)} disabled={disabled} label={pickerText.copyAs} value={value} note={`${why} ${next}`.trim()} sections={sections} selectedKey={format}
    filterLabel={pickerText.filterFormats} onAction={(key) => key === "writing-projects" ? onWritingProjects?.() : key === "paste" || key === "copy-only" ? onPasteChange?.(key) : onChange(key as Choice)} returnFocusRef={searchRef} /></>;
}
