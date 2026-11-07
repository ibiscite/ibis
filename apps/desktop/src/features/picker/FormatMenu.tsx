import type { RefObject } from "react";
import { ariaKeys, keysFor } from "../../keys";
import { isMac } from "../../platform";
import type { AppPaste } from "../../api";
import { keyOutputs, listedOutput, outputName, styleOutputs, type OutputId } from "../../format/outputs";
import { isolate, say } from "../../i18n";
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
    ...onWritingProjects ? [{ id: "project-actions", items: [{ id: "writing-projects", label: say("writing-project-menu"), command: true }] }] : [],
    ...pasteChoices && app ? [{ id: "delivery", label: say("picker-for-app", { app }), items: [
      { id: "paste", label: say("picker-paste-from-now-on", { app }), command: true },
      { id: "copy-only", label: say("picker-copy-only"), command: true },
    ] }] : [],
    { id: "key-formats", label: say("picker-key-formats"), items: keyOutputs.map((output) => (
      { id: output.id, label: output.name, sample: output.example ?? undefined })) },
    { id: "citation-styles", label: say("picker-formatted-styles"), items: styleOutputs.map((output) => (
      { id: output.id, label: output.name, keywords: output.description ?? undefined, disabled: !styles })) },
  ];
  const name = outputName(format);
  const value = writingProject ? say("picker-format-for-app", { format: name, app: writingProject.name }) : source === "held" ? say("picker-format-this-paste", { format: name }) : source === "default" || !app ? name : say("picker-format-for-app", { format: name, app });
  const why = writingProject ? say("writing-project-remembered", { name: isolate(writingProject.name) }) : source === "held" ? say("picker-format-held", { format: name }) : source === "default" ? app ? say("picker-format-app-default", { app }) : say("picker-format-no-app")
    : app ? source === "remembered" ? say("picker-format-remembered", { app }) : say("picker-format-suggested", { app }) : say("picker-format-no-app");
  const next = writingProject ? say("writing-project-pick") : source === "held" ? pick === "app" ? say("picker-pick-to-remember") : pick === "default" ? say("picker-pick-to-set-default") : ""
    : pick === "paste" ? say("picker-pick-this-paste") : pick === "default" ? say("picker-pick-default") : source === "remembered" ? say("picker-pick-replaces") : say("picker-pick-remembered");
  // One line says where a pick goes; the full reason is the menu's description.
  const heading = writingProject ? say("picker-for-app", { app: writingProject.name }) : pick === "paste" ? say("picker-pick-heading-this-paste")
    : pick === "default" || !app ? say("picker-pick-heading-default") : say("picker-for-app", { app });
  return <><span role="status" className="visually-hidden">{`${name}. ${why}`}</span><ToolbarMenu buttonRef={buttonRef} onOpenChange={onOpenChange} buttonShortcut={ariaKeys(keysFor("format"), isMac)} disabled={disabled} label={say("picker-copy-as")} value={value} heading={heading} note={`${why} ${next}`.trim()} sections={sections} selectedKey={format}
    filterLabel={say("picker-filter-formats")} onAction={(key) => {
      if (key === "writing-projects") onWritingProjects?.();
      else if (key === "paste" || key === "copy-only") onPasteChange?.(key);
      else { const output = listedOutput(key); if (output) onChange(output); }
    }} returnFocusRef={searchRef} /></>;
}
