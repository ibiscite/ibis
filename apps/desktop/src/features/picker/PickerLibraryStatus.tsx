import { useEffect, useState, type ReactNode, type RefObject } from "react";
import type { LibraryStatus, ZoteroLibrary, ZoteroProfiles } from "../../api";
import type { SystemIcon } from "../../bindings";
import { LibraryDetail, libraryCondition, LibraryStatusLine, zoteroLibraryNames } from "../../LibraryStatusLine";
import { say } from "../../i18n";
import { shortenedLabel, SourceAction, type SourceActionProps } from "../../ui/SourceAction";
import { keyLabel, keysFor } from "../../keys";
import { isMac } from "../../platform";
import { MenuItem, MenuSection, MenuSeparator } from "../../ui/Menu";
import { findSourceLabel } from "./usePickerSource";
import { libraryMessages } from "../../libraryWords";

/** The fix for these is in Zotero itself, so opening it is the action. */
export const zoteroWaits = (problem: LibraryStatus["problem"]) =>
  problem === "zotero_closed" || problem === "zotero_access_off" || problem === "zotero_profiles";

export type LibraryScope = { offered: ZoteroLibrary[]; isChosen: (choice: ZoteroLibrary) => boolean; chosenCount: number;
  name: (choice: ZoteroLibrary) => string; toggle: (choice: ZoteroLibrary) => void; all: () => void };

export function PickerLibraryStatus({ library, groupName, notice, isOpen, onOpenChange, searchRef, zotero, actions, failure, groupsFailed = false, sourceFailure, sourceAction, statusIcon, hideAction = false, rowAction, listProfiles, chooseProfile }: {
  library: LibraryStatus | null | undefined; groupName: (id: number) => string; notice?: ReactNode;
  isOpen: boolean; onOpenChange: (open: boolean) => void; searchRef: RefObject<HTMLElement | null>;
  zotero: LibraryScope | null;
  actions: { choose: () => void; find: () => void; reload: () => void; openZotero: () => void; useZotero: () => void };
  sourceAction: SourceActionProps;
  failure?: string | null;
  /** Belongs to this menu, so it shows here and never grows the footer. */
  groupsFailed?: boolean;
  sourceFailure: boolean;
  statusIcon: SystemIcon | null;
  hideAction?: boolean;
  rowAction?: string;
  listProfiles?: () => Promise<ZoteroProfiles>;
  chooseProfile?: (name: string) => void;
}) {
  // Listed when this menu opens: choosing or changing a profile lives here
  // whenever references are on screen, so it never takes them away.
  const [profiles, setProfiles] = useState<ZoteroProfiles | null>(null);
  const [profilesFailed, setProfilesFailed] = useState(false);
  const wantProfiles = isOpen && Boolean(library?.zotero) ? listProfiles : undefined;
  useEffect(() => {
    setProfilesFailed(false);
    setProfiles(null);
    if (!wantProfiles) return;
    let current = true;
    void wantProfiles().then((found) => { if (current) setProfiles(found); }, () => { if (current) setProfilesFailed(true); });
    return () => { current = false; };
  }, [wantProfiles]);
  const asked = library?.zotero && library.problem === "zotero_profiles";
  // A running Zotero answers for the profile it opened, so no choice is offered.
  const profileRows = wantProfiles && chooseProfile && profiles && !profiles.running && profiles.names.length > (asked ? 0 : 1) ? profiles : null;
  const detail = libraryMessages.detail(library, Date.now(), (status) => zoteroLibraryNames(status, groupName));
  // An optional list that fails says nothing about a healthy library.
  const condition = libraryCondition(library, { groupsFailed, profilesFailed: profilesFailed && Boolean(asked), failure });
  const openProfiles: SourceActionProps | null = asked && library.has_snapshot
    ? { ...sourceAction, icon: statusIcon, file: false, label: say("picker-choose-zotero-profile"), keys: undefined, open: () => onOpenChange(true) } : null;
  // The engine's problem code chooses the action. A closed Zotero with a
  // usable fresh snapshot has no problem and offers no interruption.
  const libraryAction = { ...sourceAction, icon: statusIcon, file: !(library?.snapshot_zotero ?? library?.zotero) };
  const failedChoice = library?.has_snapshot && library.error && !library.stale;
  const fix: SourceActionProps | null = sourceFailure ? sourceAction : openProfiles ?? (failedChoice
    ? { ...libraryAction, label: say("picker-choose-library-menu"), keys: keysFor("chooseSource"), open: actions.choose }
    : library?.error || library?.stale
    ? !library.zotero && (library.problem === "missing" || !sourceAction.file)
      ? { ...libraryAction, label: findSourceLabel(library.name), keys: library.stale && sourceAction.file ? sourceAction.keys : undefined, open: actions.find }
      // ⌘O appears only where it does the same thing: with a reference selected it means Show in Zotero.
      : library?.zotero && zoteroWaits(library.problem) ? { ...libraryAction, label: say("picker-open-zotero"), keys: sourceAction.label === say("picker-open-zotero") ? sourceAction.keys : undefined, open: actions.openZotero }
      : library?.zotero ? { ...libraryAction, label: say("picker-choose-library-menu"), keys: keysFor("chooseSource"), open: actions.choose } : sourceAction : failure ? { ...sourceAction, label: say("picker-choose-library-menu"), keys: keysFor("chooseSource"), open: actions.choose } : null);
  const detailLabel = [...detail, condition.problem].filter(Boolean).join(". ");
  return <LibraryStatusLine library={library} groupName={groupName} failure={failure} condition={condition} notice={notice}
    action={!hideAction && fix && fix.label !== rowAction && <SourceAction {...fix} />}
    menu={{ label: say("picker-library-region"), id: "library-menu",
      isOpen, onOpenChange, tabFocusRef: searchRef, placement: "top end", children: <>
        {/* The spec's order: what the menu is about (the library, its count and
            age, then any profile and the libraries searched), then what you can
            do. The detail is also the actions' group name, so a keyboard user
            hears it on entering the menu even though the heading has no items. */}
        <MenuSection id="library-detail" label={detailLabel} header={<LibraryDetail lines={detail} problem={condition.problem ?? undefined} />}>{[]}</MenuSection>
        <MenuSeparator />
        {profileRows && <MenuSection id="zotero-profile" label={say("picker-zotero-profile")} header={say("picker-zotero-profile")} layout="list" selectionMode="single">
          {profileRows.names.map((name, index) => <MenuItem key={name} id={`profile-${index}`} label={shortenedLabel(name, 36)} title={name}
            checked={name === profileRows.chosen} closeOnSelect onAction={() => chooseProfile?.(name)} />)}
        </MenuSection>}
        {profileRows && <MenuSeparator />}
        {zotero && zotero.offered.length > 1 && <MenuSection id="library-scope" label={say("picker-zotero-libraries")} header={say("picker-search-in")} layout="list">
          <MenuItem id="all" label={say("picker-all-libraries")} checked={zotero.chosenCount === zotero.offered.length} onAction={zotero.all} />
          {zotero.offered.map((choice) => {
            const checked = zotero.isChosen(choice);
            const last = checked && zotero.chosenCount === 1;
            const id = typeof choice === "object" ? `group:${choice.group}` : choice;
            const name = zotero.name(choice);
            return <MenuItem key={id} id={id} label={shortenedLabel(name, 36)} checked={checked} disabled={last}
              title={last ? say("picker-keep-one-library") : name} onAction={() => zotero.toggle(choice)} />;
          })}
        </MenuSection>}
        {zotero && zotero.offered.length > 1 && <MenuSeparator />}
        <MenuSection id="library-actions" label={detailLabel}>
          <MenuItem id="choose" label={say("picker-choose-library-menu")} keys={keyLabel(keysFor("chooseSource"), isMac)} onAction={actions.choose} />
          {!library?.zotero && <MenuItem id="zotero" label={say("picker-use-zotero")} onAction={actions.useZotero} />}
          {/* A reload can publish a recovery screen before an animated exit
              ends. Retire the modal and its same-named menu before that I/O. */}
          {library?.name && <MenuItem id="reload" label={say("picker-reload-library")} closeOnSelect onAction={actions.reload} />}
        </MenuSection>
      </> }} />;
}
