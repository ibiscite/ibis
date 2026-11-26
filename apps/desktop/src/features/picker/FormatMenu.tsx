import { useEffect, useState, type RefObject } from "react";
import { keysFor } from "../../keys";
import { isCancelled, type AppPaste } from "../../api";
import { isStyle, keyOutputs, listedOutput, outputKeywords, outputName, outputOf, previewOf, shareOutputs, sharesReferences, styleOutputs, type OutputChoice, type OutputId } from "../../format/outputs";
import { isolate, say } from "../../i18n";
import { Palette, type PaletteClose, type PaletteItem, type PaletteSection } from "../../ui/Palette";
import { PopupButton } from "../../ui/PopupButton";
import type { FormatPick, FormatSource } from "./useDestination";

/** What closed the chooser: a pick, a dismissal, or a command clearing the way. */
export type MenuCloseReason = PaletteClose;
/** How another format would show the chosen references, or null when it can't say. */
export type FormatPreview = (output: OutputId) => ReturnType<typeof previewOf>;
/** What a way to share references would copy of the shown citation, from the engine; null when it can't write them. */
export type FormatExample = (output: OutputId, signal: AbortSignal) => Promise<string | null>;

// The formats chosen in this run of Ibis, newest first: the chooser's short list.
const recent: OutputId[] = [];
/** Forget this run's recent formats (between tests, as a restart would). */
export const forgetRecentFormats = () => { recent.length = 0; };
const remember = (output: OutputId) => { recent.splice(0, recent.length, output, ...recent.filter((id) => id !== output).slice(0, 5)); };

/** The format chooser (decision 0042): the chip names the format in
 * use. Its status and palette name whose setting it is. It opens
 * the palette: the way back to your format first, your recent formats, the
 * app's own choices, then every format by typing, with a key citation or a
 * labelled style example beside the list. Focus returns to Search. */
export function FormatMenu({ format, yours, buttonRef, app, formatScope = app, rememberedFor = app, source, pick, onOpenChange, pasteChoices, onPasteChange, disabled, styles, onChange, searchRef, writing, onWritings, preview, example, usualFormat, onUseUsual, formatLabel }: {
  format: OutputId;
  formatLabel?: string;
  usualFormat?: OutputId | null;
  onUseUsual?: () => void;
  /** Your own format, the one a "back" row returns to. */
  yours?: OutputId;
  buttonRef?: RefObject<HTMLButtonElement | null>;
  /** The app the picker was opened from; owns delivery and project selection. */
  app: string | null;
  /** The site or app where the next format pick is saved. */
  formatScope?: string | null;
  /** The site or app whose saved format reset removes. */
  rememberedFor?: string | null;
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
  writing?: { name: string } | null;
  onWritings?: () => void;
  preview?: FormatPreview | null;
  example?: FormatExample | null;
}) {
  const [open, setOpen] = useState(false);
  // The pop-up names the format alone: where it goes is the bar's action, and
  // whose format it is, the palette's heading and the spoken status beside it.
  const name = formatLabel ?? outputName(format);
  const owner = source === "remembered" ? rememberedFor : formatScope;
  const why = writing ? say("writing-remembered", { name: isolate(writing.name) }) : source === "held" ? say("picker-format-held", { format: name }) : source === "default" ? formatScope ? say("picker-format-app-default", { app: formatScope }) : say("picker-format-no-app")
    : owner ? source === "remembered" ? say("picker-format-remembered", { app: owner }) : say("picker-format-suggested", { app: owner }) : source === "suggested" ? say("picker-format-suggestion") : say("picker-format-no-app");
  const next = writing ? say("writing-pick") : source === "held" ? pick === "app" ? say("picker-pick-to-remember") : pick === "default" ? say("picker-pick-to-set-default") : ""
    : pick === "paste" ? say("picker-pick-this-paste") : pick === "default" ? say("picker-pick-default") : source === "remembered" ? say("picker-pick-replaces") : say("picker-pick-remembered");
  // Whose a pick is, in the frame's footer status.
  const whose = writing ? writing.name : pick === "paste" ? say("picker-pick-heading-this-paste") : pick === "default" || !formatScope ? say("picker-pick-heading-default") : say("picker-for-app", { app: formatScope });
  const usual = usualFormat ?? yours;
  const back = app && onUseUsual && (writing || source === "remembered") ? usual ?? format : null;
  const row = (output: OutputChoice): PaletteItem => ({
    id: output.id, label: outputName(output.id), checked: output.id === format, keywords: [outputKeywords(output.id), output.example, output.shares && say("palette-words-share")].filter(Boolean).join(" "),
    ...output.style && !styles ? { disabled: true, detail: say("palette-style-unavailable") } : {},
  });
  const shortIds = [...new Set<OutputId>([format, ...recent, ...usual ? [usual] : []])].slice(0, 5);
  const shortList = shortIds.map((id) => row(outputOf(id)));
  const notRecent = (output: OutputChoice) => !shortIds.includes(output.id);
  const sections: PaletteSection[] = [
    ...back ? [{ id: "back", items: [{ id: "back", back: true, label: writing ? say("picker-stop-writing", { name: writing.name, app: app! }) : say("picker-use-usual", { format: outputName(back), app: rememberedFor! }) }] }] : [],
    { id: "recent", label: say("palette-recent"), items: shortList, only: "browsing" },
    // The app's own choices, together (decision 0042): paste into it or only copy.
    ...app && pasteChoices ? [{ id: "app", label: say("picker-for-app", { app }), items: [
      { id: "paste", label: say("picker-paste-from-now-on", { app }) }, { id: "copy-only", label: say("picker-copy-only") },
    ] }] : [],
    ...onWritings ? [{ id: "writings", items: [{ id: "writings", label: say("writing-menu") }] }] : [],
    // Each format appears once: before typing, the catalogue leaves out the recent ones above it.
    { id: "key-formats", label: say("picker-key-formats"), items: keyOutputs.filter(notRecent).map(row), only: "browsing" },
    { id: "citation-styles", label: say("picker-formatted-styles"), items: styleOutputs.filter(notRecent).map(row), only: "browsing" },
    // Sharing references is a one-off output for this copy, so it has its own group.
    { id: "share", label: say("picker-share-formats"), items: shareOutputs.filter(notRecent).map(row), only: "browsing" },
    { id: "key-formats-all", label: say("picker-key-formats"), items: keyOutputs.map(row), only: "searching" },
    { id: "citation-styles-all", label: say("picker-formatted-styles"), items: styleOutputs.map(row), only: "searching" },
    { id: "share-all", label: say("picker-share-formats"), items: shareOutputs.map(row), only: "searching" },
  ];
  // A way to share references holds for one copy whoever's format it would otherwise be.
  const once = (item: PaletteItem | undefined) => Boolean(item && !item.back && listedOutput(item.id) && sharesReferences(listedOutput(item.id)!));
  const close = (reason: PaletteClose) => {
    setOpen(false);
    onOpenChange?.(false, reason);
    // A pick, Esc or Tab returns to Search, as a pop-up does; a command clearing the way decides itself.
    if (reason !== "command") searchRef.current?.focus({ preventScroll: true });
  };
  const choose = (key: string) => {
    close("action");
    if (key === "writings") onWritings?.();
    else if (key === "back") onUseUsual?.();
    else if (key === "paste" || key === "copy-only") onPasteChange?.(key);
    else {
      const output = listedOutput(key);
      if (output) { remember(output); onChange(output); }
    }
  };
  return <>
    <span role="status" aria-label={say("picker-format-label")} className="visually-hidden">{`${name}. ${why}`}</span>
    <PopupButton ref={buttonRef} keys={keysFor("format")} isDisabled={disabled} className="toolbar-menu-button"
      aria-label={`${say("picker-format-label")} ${name}`} aria-expanded={open} aria-haspopup="dialog"
      onPress={() => { if (open) close("dismiss"); else { setOpen(true); onOpenChange?.(true); } }}
      // ↓ opens it too, as on every pop-up button.
      onKeyDown={(event) => { if (event.key === "ArrowDown" && !open && !disabled) { setOpen(true); onOpenChange?.(true); } else event.continuePropagation(); }}>{name}</PopupButton>
    {open && <Palette id="format-palette" title={say("picker-format-title")} subtitle={(item) => once(item) ? say("picker-pick-heading-this-paste") : item?.back && !writing ? say("picker-for-app", { app: rememberedFor ?? "" }) : whose} placeholder={say("palette-search-formats")}
      sections={sections} initial={back ? "back" : format} onAction={choose} onClose={close} empty={say("palette-no-formats")}
      actionLabel={(item) => item.back || listedOutput(item.id) ? say(pick === "paste" || once(item) ? "palette-use-this-paste" : pick === "default" || !formatScope ? "palette-use-default" : "palette-use-for-app", { app: writing?.name ?? (item.back ? rememberedFor : formatScope) ?? "" }) : say("palette-run")}
      description={(item) => `${why} ${once(item) ? say("picker-pick-this-paste") : next}`.trim()} preview={(item) => <FormatPreviewPane output={listedOutput(item.back ? back : item.id)} preview={preview ?? null} example={example ?? null} />} />}
  </>;
}

/** The highlighted format's citation for the chosen references: a key syntax with
 * their real key, a style by the engine's invented example, and a way to share
 * them by exactly what the engine would copy, asked without replacing the shelf's copy. */
function FormatPreviewPane({ output, preview, example }: { output: OutputId | null; preview: FormatPreview | null; example: FormatExample | null }) {
  const shown = output && preview ? preview(output) : null;
  const asks = output && example && !shown && sharesReferences(output) ? output : null;
  const [answer, setAnswer] = useState<{ output: OutputId; example: FormatExample; text: string | null }>();
  useEffect(() => {
    if (!asks || !example) return;
    const controller = new AbortController();
    example(asks, controller.signal).then(
      (text) => setAnswer({ output: asks, example, text }),
      (error) => { if (!isCancelled(error)) setAnswer({ output: asks, example, text: null }); },
    );
    return () => controller.abort();
  }, [asks, example]);
  if (!output) return null;
  const answered = asks && answer?.output === asks && answer.example === example ? answer : null;
  const text = shown ? shown.text : answered?.text ?? say(asks && !answered ? "picker-preview-pending" : "palette-preview-unavailable");
  return <>
    <h3 className="ui-list-heading ui-palette-preview-heading">{say(shown?.example ? "palette-preview-example" : "palette-preview-key")}</h3>
    <p className="ui-palette-preview-text" data-key={!isStyle(output) || undefined} dir="auto">{text}</p>
    {shown && shown.more > 0 && <p className="ui-palette-preview-note">{say("palette-preview-more", { count: shown.more })}</p>}
  </>;
}
