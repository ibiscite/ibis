import type { ReactNode, RefObject } from "react";
import type { ProjectStatus } from "../../api";
import { Icon, type IconName } from "../../icons";
import { ariaKeys, keyLabel, keysFor } from "../../keys";
import { isolate, say } from "../../i18n";
import { currentPlatform } from "../../platform";
import { Button } from "../../ui/Button";
import { IconButton } from "../../ui/IconButton";
import { KeyHint } from "../../ui/KeyCaps";
import { Palette, type PaletteItem, type PaletteSection } from "../../ui/Palette";
import { SourceIcon, type SourceActionProps } from "../../ui/SourceAction";
import { COMMAND_ICONS } from "./commands";
import { projectDetail } from "./useProjectBibliography";

type Action = PaletteItem & { run: () => void; icon: ReactNode };

/** More (⌘K) is the actions palette: every action in the picker, found by its
 * name or a common word for it, each showing its key (control placement, the
 * third door). Grouped by what it acts on: the library, the citation, the
 * window, Ibis. Preferences live in Settings and the format chooser holds the
 * app's own choices. While Keep Window Open is on, the footer shows it. */
export function PickerMenu({ isOpen, onOpenChange, buttonRef, searchRef, libraryDetail, lastOutcome, pinned, keepOrder, citation, actions, source, project }: {
  /** Acts on the focused citation token, or the last one. */
  citation?: { tokens: number; selected: number; focus: () => void; edit: () => void; move: (step: -1 | 1) => void; remove: () => void } | null;
  source?: SourceActionProps;
  isOpen: boolean; onOpenChange: (open: boolean) => void;
  buttonRef: RefObject<HTMLButtonElement | null>; searchRef: RefObject<HTMLElement | null>;
  /** Where the references come from and how fresh they are. */
  libraryDetail: [string, string | null] | null;
  lastOutcome?: string | null;
  pinned: boolean;
  /** Offered only where the output may sort: a style. */
  keepOrder: { checked: boolean; toggle: () => void } | null;
  actions: { pin: () => void; keys: () => void; settings: (() => void) | null; format: (() => void) | null;
    chooseSource: () => void; revealSource: (() => void) | null;
    /** Offered once this app has a citation from this run of Ibis. */
    changeLast?: (() => void) | null;
    /** The highlighted reference's details, as ⌘Y shows them. */
    peek?: (() => void) | null;
    forgetRecent?: (() => void) | null; quit: () => void;
    sessionReferences?: (() => void) | null };
  /** The bibliography keys are checked against, and the entry of the highlighted row. */
  project?: { status: ProjectStatus | null; choose: (() => void) | null; stop: (() => void) | null; copyEntry: (() => void) | null };
}) {
  const platform = currentPlatform();
  // Read now, not at load: the platform decides the keys people see.
  const mac = platform === "macos";
  // Every action has a glyph on the tile column: the icon is not optional.
  const action = (id: string, label: string, icon: IconName | Exclude<ReactNode, string>, run: (() => void) | null | undefined, more: Partial<Action> = {}): Action[] =>
    run ? [{ id, label, run, icon: typeof icon === "string" ? <Icon name={icon as IconName} /> : icon, keywords: say(`palette-words-${id}` as never), ...more }] : [];
  const keyed = (id: Parameters<typeof keysFor>[0]) => ({ keys: keysFor(id, mac) || undefined });
  const showSource = say("picker-show-source-folder", { platform: mac ? "macos" : "other" });
  const sections: (PaletteSection & { items: Action[] })[] = [
    // The footer's status already names the library under More; the group's name
    // says it again only for screen readers, with the last outcome first.
    { id: "library", label: say("palette-library"), name: [lastOutcome && say("picker-last-outcome", { outcome: lastOutcome }), libraryDetail?.filter(Boolean).join(". ")].filter(Boolean).join(". ") || undefined, items: [
      ...source ? action("source", source.label, <SourceIcon icon={source.icon} file={source.file} glyph={source.glyph} />, source.open, { keys: source.keys, detail: lastOutcome ?? undefined }) : [],
      ...action("reveal-source", showSource, COMMAND_ICONS.revealSource, source?.label === showSource ? null : actions.revealSource, keyed("revealSource")),
      ...action("choose-source", say("picker-choose-library-menu"), COMMAND_ICONS.chooseSource, actions.chooseSource, keyed("chooseSource")),
    ] },
    // The bibliography keys are checked against, named by its state.
    { id: "project", label: project?.status ? projectDetail(project.status) : undefined, items: [
      ...project?.choose && project.copyEntry && project.status ? action("project-entry", say("picker-copy-project-entry", { name: isolate(project.status.name) }), "file", project.copyEntry) : [],
      ...project?.choose ? action("project-choose", project.status ? say("picker-choose-other-project") : say("picker-choose-project"), "check-file", project.choose) : [],
      ...project?.status && project.stop ? action("project-stop", say("picker-stop-project", { name: isolate(project.status.name) }), "close", project.stop) : [],
    ] },
    { id: "citation", label: say("palette-citation"), items: [
      ...action("format", say("picker-change-format"), COMMAND_ICONS.format, actions.format, keyed("format")),
      ...action("peek", say("palette-peek"), "peek", actions.peek, keyed("peek")),
      ...action("change-last", say("picker-change-last-citation"), COMMAND_ICONS.changeLast, actions.changeLast, keyed("changeLast")),
      ...citation?.tokens ? [
        ...action("citation-focus", say("key-go-to-citation"), "arrow-down", citation.focus, keyed("citationFocus")),
        ...action("citation-edit", say("key-edit-citation"), "hash", citation.edit),
      ] : [],
      ...citation && citation.selected > 1 ? [
        ...action("citation-move-left", say("key-move-left"), "arrow-left", () => citation.move(-1), { keys: "Shift+ArrowLeft" }),
        ...action("citation-move-right", say("key-move-right"), "arrow-right", () => citation.move(1), { keys: "Shift+ArrowRight" }),
      ] : [],
      ...citation && citation.selected > 0 ? action("citation-remove", say("key-remove-from-citation"), "minus", citation.remove, { keys: "Backspace" }) : [],
      ...keepOrder ? action("keep-order", say("picker-keep-my-order"), "order", keepOrder.toggle, { checked: keepOrder.checked }) : [],
      ...action("session-references", say("session-menu"), "history", actions.sessionReferences),
      ...action("forget-recent", say("picker-forget-recent"), "forget", actions.forgetRecent),
    ] },
    { id: "ibis", label: say("palette-ibis"), items: [
      ...action("pin", say("picker-keep-open-menu"), COMMAND_ICONS.pin, actions.pin, { ...keyed("pin"), checked: pinned }),
      ...action("keys", say("key-title"), COMMAND_ICONS.keys, actions.keys, keyed("keys")),
      ...action("settings", say("picker-settings"), COMMAND_ICONS.settings, actions.settings, keyed("settings")),
      // Closing the window only hides it; this ends the app and frees the shortcut.
      ...action("quit", say("picker-quit-label"), COMMAND_ICONS.quit, actions.quit, keyed("quit")),
    ] },
  ];
  const run = (id: string) => sections.flatMap((section) => section.items).find((item) => item.id === id)?.run();
  const more = keysFor("more");
  return <>
    {pinned && <IconButton icon="pin" className="footer-pinned" isSelected aria-label={say("picker-keep-open")}
      title={`${say("picker-keep-open-menu")} (${keyLabel(keysFor("pin"), mac)})`} keys={keysFor("pin")} onChange={actions.pin} />}
    <Button ref={buttonRef} variant="quiet" size="small" className="more-button" aria-label={say("picker-more")} aria-haspopup="dialog" aria-expanded={isOpen}
      title={say("picker-more-title", { keys: keyLabel(more, mac) })} aria-keyshortcuts={ariaKeys(more, mac)} onPress={() => onOpenChange(!isOpen)}>
      <KeyHint label={say("picker-more")} keys={more} platform={platform} hidden />
    </Button>
    {isOpen && <Palette id="actions-palette" title={say("palette-actions-title")} invoker={buttonRef}
      placeholder={say("palette-search-actions")} sections={sections} empty={say("palette-no-actions")}
      onAction={(id) => { onOpenChange(false); searchRef.current?.focus({ preventScroll: true }); run(id); }}
      onClose={(reason) => { onOpenChange(false); if (reason === "tab") searchRef.current?.focus({ preventScroll: true }); }}
      actionLabel={() => say("palette-run")} />}
  </>;
}
