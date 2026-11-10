import type { Ref, RefObject } from "react";
import type { ProjectStatus } from "../../api";
import { Icon } from "../../icons";
import { ariaKeys, keyLabel, keysFor } from "../../keys";
import { isolate, say } from "../../i18n";
import { currentPlatform } from "../../platform";
import { Button } from "../../ui/Button";
import { IconButton } from "../../ui/IconButton";
import { KeyHint } from "../../ui/KeyCaps";
import { Palette, type PaletteItem, type PaletteSection } from "../../ui/Palette";
import { SourceIcon, type SourceActionProps } from "../../ui/SourceAction";
import { projectDetail } from "./useProjectBibliography";

type Action = PaletteItem & { run: () => void };

/** More (⌘K) is the actions palette: every action in the picker, found by its
 * name or a common word for it, each showing its key (control placement, the
 * third door). Grouped by what it acts on: the library, the citation, the
 * window, Ibis. Preferences live in Settings and the format chooser holds the
 * app's own choices. While Keep Window Open is on, the footer shows it. */
export function PickerMenu({ isOpen, onOpenChange, buttonRef, searchRef, libraryDetail, lastOutcome, pinned, keepOrder, actions, source, project }: {
  source?: SourceActionProps;
  isOpen: boolean; onOpenChange: (open: boolean) => void;
  buttonRef: Ref<HTMLButtonElement>; searchRef: RefObject<HTMLElement | null>;
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
  const action = (id: string, label: string, run: (() => void) | null | undefined, more: Partial<Action> = {}): Action[] =>
    run ? [{ id, label, run, keywords: say(`palette-words-${id}` as never), ...more }] : [];
  const keyed = (command: Parameters<typeof keysFor>[0]) => ({ keys: keysFor(command) });
  const sections: (PaletteSection & { items: Action[] })[] = [
    // The footer's status already names the library under More; the group's name
    // says it again only for screen readers, with the last outcome first.
    { id: "library", label: say("palette-library"), name: [lastOutcome && say("picker-last-outcome", { outcome: lastOutcome }), libraryDetail?.filter(Boolean).join(". ")].filter(Boolean).join(". ") || undefined, items: [
      ...source ? action("source", source.label, source.open, { icon: <SourceIcon icon={source.icon} file={source.file} />, keys: source.keys, detail: lastOutcome ?? undefined }) : [],
      ...action("reveal-source", say("picker-show-source-folder", { platform: mac ? "macos" : "other" }), source?.label === say("picker-show-source-folder", { platform: mac ? "macos" : "other" }) ? null : actions.revealSource, { icon: <Icon name="find" />, ...keyed("revealSource") }),
      ...action("choose-source", say("picker-choose-library-menu"), actions.chooseSource, { icon: <Icon name="library" />, ...keyed("chooseSource") }),
    ] },
    // The bibliography keys are checked against, named by its state.
    { id: "project", label: project?.status ? projectDetail(project.status) : undefined, items: [
      ...project?.choose && project.copyEntry && project.status ? action("project-entry", say("picker-copy-project-entry", { name: isolate(project.status.name) }), project.copyEntry, { icon: <Icon name="file" /> }) : [],
      ...project?.choose ? action("project-choose", project.status ? say("picker-choose-other-project") : say("picker-choose-project"), project.choose, { icon: <Icon name="book" /> }) : [],
      ...project?.status && project.stop ? action("project-stop", say("picker-stop-project", { name: isolate(project.status.name) }), project.stop, { icon: <Icon name="close" /> }) : [],
    ] },
    { id: "citation", label: say("palette-citation"), items: [
      ...action("format", say("picker-change-format"), actions.format, { icon: <Icon name="swap" />, ...keyed("format") }),
      ...action("peek", say("palette-peek"), actions.peek, { icon: <Icon name="look" />, ...keyed("peek") }),
      ...action("change-last", say("picker-change-last-citation"), actions.changeLast, { icon: <Icon name="write" />, ...keyed("changeLast") }),
      ...keepOrder ? action("keep-order", say("picker-keep-my-order"), keepOrder.toggle, { checked: keepOrder.checked, icon: <Icon name="chevrons" /> }) : [],
      ...action("session-references", say("session-menu"), actions.sessionReferences, { icon: <Icon name="citations" /> }),
      ...action("forget-recent", say("picker-forget-recent"), actions.forgetRecent, { icon: <Icon name="retry" /> }),
    ] },
    { id: "ibis", label: say("palette-ibis"), items: [
      ...action("pin", say("picker-keep-open-menu"), actions.pin, { checked: pinned, icon: <Icon name="pin" />, ...keyed("pin") }),
      ...action("keys", say("key-title"), actions.keys, { icon: <Icon name="shortcuts" />, ...keyed("keys") }),
      ...action("settings", say("picker-settings"), actions.settings, { icon: <Icon name="general" />, ...keyed("settings") }),
      // Closing the window only hides it; this ends the app and frees the shortcut.
      ...action("quit", say("picker-quit-label"), actions.quit, { icon: <Icon name="close" />, ...keyed("quit") }),
    ] },
  ];
  const run = (id: string) => sections.flatMap((section) => section.items).find((item) => item.id === id)?.run();
  const more = keysFor("more");
  return <>
    {pinned && <IconButton icon="pin" className="footer-pinned" isSelected aria-label={say("picker-keep-open")}
      title={`${say("picker-keep-open-menu")} (${keyLabel(keysFor("pin"), mac)})`} aria-keyshortcuts={ariaKeys(keysFor("pin"), mac)} onChange={actions.pin} />}
    <Button ref={buttonRef} variant="quiet" size="small" className="more-button" aria-label={say("picker-more")} aria-haspopup="dialog" aria-expanded={isOpen}
      title={say("picker-more-title", { keys: keyLabel(more, mac) })} aria-keyshortcuts={ariaKeys(more, mac)} onPress={() => onOpenChange(!isOpen)}>
      <KeyHint label={say("picker-more")} keys={more} platform={platform} hidden />
    </Button>
    {isOpen && <Palette id="actions-palette" title={say("palette-actions-title")}
      placeholder={say("palette-search-actions")} sections={sections} empty={say("palette-no-actions")}
      onAction={(id) => { onOpenChange(false); searchRef.current?.focus({ preventScroll: true }); run(id); }}
      onClose={(reason) => { onOpenChange(false); if (reason === "tab") searchRef.current?.focus({ preventScroll: true }); }}
      actionLabel={() => say("palette-run")} />}
  </>;
}
