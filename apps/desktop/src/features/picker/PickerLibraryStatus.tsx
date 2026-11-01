import type { RefObject } from "react";
import type { LibraryStatus, ZoteroLibrary } from "../../api";
import type { SystemIcon } from "../../bindings";
import { LibraryDetail, LibraryStatusLine, zoteroLibraryNames } from "../../LibraryStatusLine";
import { libraryMessages, pickerText } from "../../messages";
import { SourceAction, type SourceActionProps } from "../../ui/SourceAction";
import { keyLabel, keysFor } from "../../keys";
import { isMac } from "../../platform";
import { MenuItem, MenuSection } from "../../ui/Menu";

export type LibraryScope = { offered: ZoteroLibrary[]; isChosen: (choice: ZoteroLibrary) => boolean; chosenCount: number;
  name: (choice: ZoteroLibrary) => string; toggle: (choice: ZoteroLibrary) => void };

export function PickerLibraryStatus({ library, groupName, isOpen, onOpenChange, searchRef, zotero, actions, failure, sourceFailure, sourceAction, statusIcon }: {
  library: LibraryStatus | null | undefined; groupName: (id: number) => string;
  isOpen: boolean; onOpenChange: (open: boolean) => void; searchRef: RefObject<HTMLElement | null>;
  zotero: LibraryScope | null;
  actions: { choose: () => void; find: () => void; reload: () => void; openZotero: () => void; useZotero: () => void };
  sourceAction: SourceActionProps;
  failure?: string | null;
  sourceFailure: boolean;
  statusIcon: SystemIcon | null;
}) {
  const detail = libraryMessages.detail(library, Date.now(), (status) => zoteroLibraryNames(status, groupName));
  // The engine's problem code chooses the action. A closed Zotero with a
  // usable fresh snapshot has no problem and offers no interruption.
  const libraryAction = { ...sourceAction, icon: statusIcon, file: !(library?.snapshot_zotero ?? library?.zotero) };
  const failedChoice = library?.has_snapshot && library.error && !library.stale;
  const fix: SourceActionProps | null = sourceFailure ? sourceAction : failedChoice
    ? { ...libraryAction, label: pickerText.chooseLibraryMenu, keys: keysFor("chooseSource"), open: actions.choose }
    : library?.error || library?.stale
    ? !library.zotero && (library.problem === "missing" || !sourceAction.file)
      ? { ...libraryAction, label: pickerText.findSourceFile(library.name), keys: library.stale && sourceAction.file ? sourceAction.keys : undefined, open: actions.find }
      // ⌘O appears only where it does the same thing: with a reference selected it means Show in Zotero.
      : library?.zotero ? { ...libraryAction, label: pickerText.openZotero, keys: sourceAction.label === pickerText.openZotero ? sourceAction.keys : undefined, open: actions.openZotero }
      : sourceAction : failure ? { ...sourceAction, label: pickerText.chooseLibraryMenu, keys: keysFor("chooseSource"), open: actions.choose } : null;
  return <LibraryStatusLine library={library} groupName={groupName} failure={failure}
    action={fix && <SourceAction {...fix} />}
    menu={{ label: pickerText.libraryRegion, id: "library-menu",
      isOpen, onOpenChange, tabFocusRef: searchRef, placement: "top end", children: <>
        <MenuSection id="library-detail" label={detail.filter(Boolean).join(". ")} header={<LibraryDetail lines={detail} />}>
          {/* A reload can publish a recovery screen before an animated exit
              ends. Retire the modal and its same-named menu before that I/O. */}
          {library?.name && <MenuItem id="reload" label={pickerText.reloadLibrary} closeOnSelect onAction={actions.reload} />}
          <MenuItem id="choose" label={pickerText.chooseLibraryMenu} keys={keyLabel(keysFor("chooseSource"), isMac)} onAction={actions.choose} />
          {!library?.zotero && <MenuItem id="zotero" label={pickerText.useZotero} onAction={actions.useZotero} />}
        </MenuSection>
        {zotero && zotero.offered.length > 1 && <MenuSection id="library-scope" label={pickerText.zoteroLibraries} layout="chips">
          {zotero.offered.map((choice) => {
            const checked = zotero.isChosen(choice);
            const last = checked && zotero.chosenCount === 1;
            const id = typeof choice === "object" ? `group:${choice.group}` : choice;
            return <MenuItem key={id} id={id} label={zotero.name(choice)} checked={checked} disabled={last}
              title={last ? pickerText.keepOneLibrary : undefined} onAction={() => zotero.toggle(choice)} />;
          })}
        </MenuSection>}
      </> }} />;
}
