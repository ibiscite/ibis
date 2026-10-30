import type { RefObject } from "react";
import type { LibraryStatus, ZoteroLibrary } from "../../api";
import { LibraryDetail, LibraryStatusLine, zoteroLibraryNames } from "../../LibraryStatusLine";
import { libraryMessages, pickerText } from "../../messages";
import { Button } from "../../ui/Button";
import { MenuItem, MenuSection } from "../../ui/Menu";

export type LibraryScope = { offered: ZoteroLibrary[]; isChosen: (choice: ZoteroLibrary) => boolean; chosenCount: number;
  name: (choice: ZoteroLibrary) => string; toggle: (choice: ZoteroLibrary) => void };

export function PickerLibraryStatus({ library, groupName, isOpen, onOpenChange, searchRef, zotero, actions, failure }: {
  library: LibraryStatus | null | undefined; groupName: (id: number) => string;
  isOpen: boolean; onOpenChange: (open: boolean) => void; searchRef: RefObject<HTMLElement | null>;
  zotero: LibraryScope | null;
  actions: { choose: () => void; reload: () => void; openZotero: () => void; useZotero: () => void };
  failure?: string | null;
}) {
  const detail = libraryMessages.detail(library, Date.now(), (status) => zoteroLibraryNames(status, groupName));
  // The engine's problem code chooses the action. A closed Zotero with a
  // usable fresh snapshot has no problem and offers no interruption.
  const fix = library?.error || library?.stale ? library?.problem === "missing" && !library.zotero
    ? { label: pickerText.findSource, run: actions.choose }
    : library?.zotero ? { label: pickerText.openZotero, run: actions.openZotero }
    : { label: pickerText.reload, run: actions.reload } : null;
  return <LibraryStatusLine library={library} groupName={groupName} failure={failure}
    action={fix && <Button variant="quiet" size="small" className="library-inline-action" onPress={fix.run}>{fix.label}</Button>}
    menu={{ label: pickerText.libraryRegion, id: "library-menu",
      isOpen, onOpenChange, tabFocusRef: searchRef, placement: "top end", children: <>
        <MenuSection id="library-detail" label={detail.filter(Boolean).join(". ")} header={<LibraryDetail lines={detail} />}>
          {library?.name && <MenuItem id="reload" label={pickerText.reloadLibrary} onAction={actions.reload} />}
          <MenuItem id="choose" label={pickerText.chooseLibraryMenu} onAction={actions.choose} />
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
