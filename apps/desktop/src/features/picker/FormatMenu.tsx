import { useState, type RefObject } from "react";
import { ariaKeys, keysFor } from "../../keys";
import { isMac } from "../../platform";
import type { AppPaste } from "../../api";
import { isStyle, keyOutputs, listedOutput, outputName, outputOf, previewOf, styleOutputs, type OutputChoice, type OutputId } from "../../format/outputs";
import { isolate, say } from "../../i18n";
import { Palette, type PaletteClose, type PaletteItem, type PaletteSection } from "../../ui/Palette";
import { PopupButton } from "../../ui/PopupButton";
import type { FormatPick, FormatSource } from "./useDestination";

/** What closed the chooser: a pick, a dismissal, or a command clearing the way. */
export type MenuCloseReason = PaletteClose;
/** How another format would show the chosen references, or null when it can't say. */
export type FormatPreview = (output: OutputId) => ReturnType<typeof previewOf>;

// The formats chosen in this run of Ibis, newest first: the chooser's short list.
const recent: OutputId[] = [];
/** Forget this run's recent formats (between tests, as a restart would). */
export const forgetRecentFormats = () => { recent.length = 0; };
const remember = (output: OutputId) => { recent.splice(0, recent.length, output, ...recent.filter((id) => id !== output).slice(0, 5)); };

/** The format chooser (decision 0042, palette B): the chip names the format in
 * use and, when it isn't your own, why ("APA · Word", "APA · Thesis"). It opens
 * the palette: the way back to your format first, your recent formats, the
 * app's own choices, then every format by typing, with the highlighted
 * format's real citation beside the list. Focus returns to Search. */
export function FormatMenu({ format, yours, buttonRef, app, source, pick, onOpenChange, pasteChoices, onPasteChange, disabled, styles, onChange, searchRef, writingProject, onWritingProjects, preview }: {
  format: OutputId;
  /** Your own format, the one a "back" row returns to. */
  yours?: OutputId;
  buttonRef?: RefObject<HTMLButtonElement | null>;
  /** The app the picker was opened from, named beside the format. */
  app: string | null;
  /** Why the next paste uses `format`. */
  source: FormatSource;
  /** Where a pick goes. */
  pick: FormatPick;
  onOpenChange?: (open: boolean, reason?: MenuCloseReason) => void;
  /** The app's paste question is open: offer its two answers. */
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
  preview?: FormatPreview | null;
}) {
  const [open, setOpen] = useState(false);
  const name = outputName(format);
  const value = writingProject ? say("picker-format-for-app", { format: name, app: writingProject.name }) : source === "held" ? say("picker-format-this-paste", { format: name }) : source === "default" || !app ? name : say("picker-format-for-app", { format: name, app });
  const why = writingProject ? say("writing-project-remembered", { name: isolate(writingProject.name) }) : source === "held" ? say("picker-format-held", { format: name }) : source === "default" ? app ? say("picker-format-app-default", { app }) : say("picker-format-no-app")
    : app ? source === "remembered" ? say("picker-format-remembered", { app }) : say("picker-format-suggested", { app }) : say("picker-format-no-app");
  const next = writingProject ? say("writing-project-pick") : source === "held" ? pick === "app" ? say("picker-pick-to-remember") : pick === "default" ? say("picker-pick-to-set-default") : ""
    : pick === "paste" ? say("picker-pick-this-paste") : pick === "default" ? say("picker-pick-default") : source === "remembered" ? say("picker-pick-replaces") : say("picker-pick-remembered");
  // Whose a pick is, quietly beside the palette's name.
  const whose = writingProject ? writingProject.name : pick === "paste" ? say("picker-pick-heading-this-paste") : pick === "default" || !app ? say("picker-pick-heading-default") : say("picker-for-app", { app });
  const back = !writingProject && app && yours && yours !== format && (source === "remembered" || source === "suggested") ? yours : null;
  const row = (output: OutputChoice): PaletteItem => ({
    id: output.id, label: output.name, checked: output.id === format, keywords: [output.example, output.description].filter(Boolean).join(" "),
    ...output.style && !styles ? { disabled: true, detail: say("palette-style-unavailable") } : {},
  });
  const shortIds = [...new Set<OutputId>([format, ...recent, ...yours ? [yours] : []])].slice(0, 5);
  const shortList = shortIds.map((id) => row(outputOf(id)));
  const notRecent = (output: OutputChoice) => !shortIds.includes(output.id);
  const sections: PaletteSection[] = [
    ...back ? [{ id: "back", items: [{ id: "back", back: true, label: say("palette-use-again", { format: outputName(back), app: app! }) }] }] : [],
    { id: "recent", label: say("palette-recent"), items: shortList, only: "browsing" },
    // The app's own choices, together (decision 0042): paste into it or only copy.
    ...app && pasteChoices ? [{ id: "app", label: say("picker-for-app", { app }), items: [
      { id: "paste", label: say("picker-paste-from-now-on", { app }) }, { id: "copy-only", label: say("picker-copy-only") },
    ] }] : [],
    ...onWritingProjects ? [{ id: "writings", items: [{ id: "writing-projects", label: say("writing-project-menu") }] }] : [],
    // Each format appears once: before typing, the catalogue leaves out the recent ones above it.
    { id: "key-formats", label: say("picker-key-formats"), items: keyOutputs.filter(notRecent).map(row), only: "browsing" },
    { id: "citation-styles", label: say("picker-formatted-styles"), items: styleOutputs.filter(notRecent).map(row), only: "browsing" },
    { id: "key-formats-all", label: say("picker-key-formats"), items: keyOutputs.map(row), only: "searching" },
    { id: "citation-styles-all", label: say("picker-formatted-styles"), items: styleOutputs.map(row), only: "searching" },
  ];
  const close = (reason: PaletteClose) => {
    setOpen(false);
    onOpenChange?.(false, reason);
    // A pick, Esc or Tab returns to Search, as a pop-up does; a command clearing the way decides itself.
    if (reason !== "command") searchRef.current?.focus({ preventScroll: true });
  };
  const choose = (key: string) => {
    close("action");
    if (key === "writing-projects") onWritingProjects?.();
    else if (key === "paste" || key === "copy-only") onPasteChange?.(key);
    else {
      const output = listedOutput(key === "back" ? back : key);
      if (output) { remember(output); onChange(output); }
    }
  };
  return <>
    <span role="status" aria-label={say("picker-format-label")} className="visually-hidden">{`${name}. ${why}`}</span>
    <PopupButton ref={buttonRef} aria-keyshortcuts={ariaKeys(keysFor("format"), isMac)} isDisabled={disabled} className="toolbar-menu-button"
      aria-label={`${say("picker-format-label")} ${value}`} aria-expanded={open} aria-haspopup="dialog"
      onPress={() => { if (open) close("dismiss"); else { setOpen(true); onOpenChange?.(true); } }}
      // ↓ opens it too, as on every pop-up button.
      onKeyDown={(event) => { if (event.key === "ArrowDown" && !open && !disabled) { setOpen(true); onOpenChange?.(true); } else event.continuePropagation(); }}>{value}</PopupButton>
    {open && <Palette id="format-palette" title={say("picker-format-title")} subtitle={whose} placeholder={say("palette-search-formats", { count: keyOutputs.length + styleOutputs.length })}
      sections={sections} initial={back ? "back" : format} onAction={choose} onClose={close} empty={say("palette-no-formats")}
      actionLabel={(item) => item.back || listedOutput(item.id) ? say(pick === "paste" ? "palette-use-this-paste" : pick === "default" || !app ? "palette-use-default" : "palette-use-for-app", { app: writingProject?.name ?? app ?? "" }) : say("palette-run")}
      description={`${why} ${next}`.trim()} preview={(item) => <FormatPreviewPane output={listedOutput(item.back ? back : item.id)} preview={preview ?? null} />} />}
  </>;
}

/** The highlighted format's citation for the chosen references: a key syntax with
 * their real key, a style by the engine's invented example (only the engine
 * formats real references, and asking it here would replace the shelf's copy). */
function FormatPreviewPane({ output, preview }: { output: OutputId | null; preview: FormatPreview | null }) {
  const shown = output && preview ? preview(output) : null;
  if (!output) return null;
  return <>
    <h3 className="ui-palette-preview-heading">{say(shown?.example ? "palette-preview-example" : "palette-preview-key")}</h3>
    <p className="ui-palette-preview-text" data-key={!isStyle(output) || undefined} dir="auto">
      {shown ? shown.text : say("palette-preview-unavailable")}
    </p>
    {shown && shown.more > 0 && <p className="ui-palette-preview-note">{say("palette-preview-more", { count: shown.more })}</p>}
  </>;
}
