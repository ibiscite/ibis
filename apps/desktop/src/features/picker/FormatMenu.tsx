import type { RefObject } from "react";
import { ariaKeys, keysFor } from "../../keys";
import { isMac } from "../../platform";
import type { AppPaste } from "../../api";
import { keyOutputs, listedOutput, outputName, styleOutputs, type OutputId } from "../../format/outputs";
import { pickerText, writingProjectText } from "../../messages";
import { ToolbarMenu, type ToolbarMenuSection, type MenuCloseReason } from "../../ui/ToolbarMenu";
import type { FormatPick, FormatSource } from "./useDestination";

/** The copy format menu: the engine's key syntaxes and common styles, each
 * by its one name, with what it writes and a style's full title beside it. Picking one remembers it for the app; without an app it is the
 * default. The menu says which applies and why.
 * Styles stay listed but unavailable while the engine is not offered.
 * Focus goes back to the search box after a pick, Escape or Tab. */
export function FormatMenu({ format, buttonRef, app, source, pick, onOpenChange, pasteChoices, onPasteChange, disabled, styles, onChange, searchRef, writingProject, onWritingProjects }: {
  format: OutputId;
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
  onChange: (output: OutputId) => void;
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
    { id: "key-formats", label: pickerText.keyFormats, items: keyOutputs.map((output) => (
      { id: output.id, label: output.name, sample: output.example ?? undefined })) },
    { id: "citation-styles", label: pickerText.formattedStyles, items: styleOutputs.map((output) => (
      { id: output.id, label: output.name, detail: output.description ?? undefined, sample: output.example ?? undefined, prose: true, disabled: !styles })) },
  ];
  const name = outputName(format);
  const value = writingProject ? pickerText.formatForApp(name, writingProject.name) : source === "held" ? pickerText.formatThisPaste(name) : source === "default" || !app ? pickerText.formatDefault(name) : pickerText.formatForApp(name, app);
  const why = writingProject ? writingProjectText.remembered(writingProject.name) : source === "held" ? pickerText.formatHeld(name) : source === "default" ? app ? pickerText.formatAppDefault(app) : pickerText.formatNoApp
    : app ? source === "remembered" ? pickerText.formatRemembered(app) : pickerText.formatSuggested(app) : pickerText.formatNoApp;
  const next = writingProject ? writingProjectText.pick : source === "held" ? pick === "app" ? pickerText.pickToRemember : pick === "default" ? pickerText.pickToSetDefault : ""
    : pick === "paste" ? pickerText.pickThisPaste : pick === "default" ? pickerText.pickDefault : source === "remembered" ? pickerText.pickReplaces : pickerText.pickRemembered;
  return <><span role="status" className="visually-hidden">{`${name}. ${why}`}</span><ToolbarMenu buttonRef={buttonRef} onOpenChange={onOpenChange} buttonShortcut={ariaKeys(keysFor("format"), isMac)} disabled={disabled} label={pickerText.copyAs} value={value} note={`${why} ${next}`.trim()} sections={sections} selectedKey={format}
    filterLabel={pickerText.filterFormats} onAction={(key) => {
      if (key === "writing-projects") onWritingProjects?.();
      else if (key === "paste" || key === "copy-only") onPasteChange?.(key);
      else { const output = listedOutput(key); if (output) onChange(output); }
    }} returnFocusRef={searchRef} /></>;
}
