import type { Ref, RefObject } from "react";
import type { Density, LibraryStatus, LoginItem, ZoteroLibrary } from "../../api";
import { ariaKeys, keyLabel, keysFor } from "../../keys";
import { LibraryDetail } from "../../LibraryStatusLine";
import { densityNames, keyText, loginItemText, pickerText } from "../../messages";
import { currentPlatform, isMac } from "../../platform";
import { MenuItem, MenuSection, MenuSeparator, MoreMenu } from "./MoreMenu";

const label = (accelerator: string) => keyLabel(accelerator, isMac);

/** The More button and menu: the library and its Zotero scope, view settings,
 * starting at login, the keys overlay and Quit. */
export function PickerMenu({ isOpen, onOpenChange, buttonRef, searchRef, library, libraryDetail, zotero, density, largerText, startAtLogin, actions }: {
  isOpen: boolean; onOpenChange: (open: boolean) => void;
  buttonRef: Ref<HTMLButtonElement>; searchRef: RefObject<HTMLElement | null>;
  library: LibraryStatus | null | undefined;
  /** Where the references come from and how fresh they are. */
  libraryDetail: [string, string | null] | null;
  /** The Zotero libraries to offer, when the library is Zotero and can be narrowed. */
  zotero: { offered: ZoteroLibrary[]; isChosen: (choice: ZoteroLibrary) => boolean; chosenCount: number; name: (choice: ZoteroLibrary) => string; toggle: (choice: ZoteroLibrary) => void } | null;
  density: Density;
  largerText: boolean;
  /** Offered when the system supports it. */
  startAtLogin: { item: LoginItem; toggle: () => void } | null;
  actions: { choose: () => void; useZotero: () => void; reload: () => void; density: () => void; largerText: () => void; keys: () => void; quit: () => void };
}) {
  return (
    <MoreMenu isOpen={isOpen} onOpenChange={onOpenChange} buttonRef={buttonRef} searchRef={searchRef}
      keys={label(keysFor("more"))} ariaKeys={ariaKeys(keysFor("more"), isMac)}>
      {/* Where the references come from and how fresh they are, read
          as the group's name when the menu is reached by keyboard. */}
      <MenuSection id="library" label={libraryDetail?.filter(Boolean).join(". ")}
        header={libraryDetail && <LibraryDetail lines={libraryDetail} />}>
        <MenuItem id="choose" label={pickerText.chooseLibrary} title={pickerText.chooseTitle} onAction={actions.choose} />
        {!library?.zotero && (
          <MenuItem id="zotero" label={pickerText.useZotero} title={pickerText.useZoteroTitle} onAction={actions.useZotero} />
        )}
        {library?.name && (
          <MenuItem id="reload" label={pickerText.reload} title={pickerText.reloadTitle} onAction={actions.reload} />
        )}
      </MenuSection>
      {zotero && (
        <MenuSection id="zotero-libraries" label={pickerText.zoteroLibraries}>
          {zotero.offered.map((choice) => {
            const checked = zotero.isChosen(choice);
            const last = checked && zotero.chosenCount === 1;
            const id = typeof choice === "object" ? `group:${choice.group}` : choice;
            return (
              <MenuItem key={id} id={id} label={zotero.name(choice)}
                title={last ? pickerText.keepOneLibrary : undefined} checked={checked} disabled={last}
                onAction={() => zotero.toggle(choice)} />
            );
          })}
        </MenuSection>
      )}
      <MenuSeparator />
      <MenuSection id="view">
        <MenuItem id="density" label={pickerText.density(densityNames[density])} title={pickerText.densityTitle} onAction={actions.density} />
      </MenuSection>
      <MenuSection id="view-toggles">
        <MenuItem id="larger-text" label={pickerText.largerText} checked={largerText} onAction={actions.largerText} />
        {startAtLogin && (
          <MenuItem id="start-at-login" label={loginItemText.label(currentPlatform())} title={loginItemText.title(currentPlatform())} checked={startAtLogin.item.enabled} onAction={startAtLogin.toggle} />
        )}
      </MenuSection>
      <MenuSeparator />
      <MenuSection id="ibis">
        <MenuItem id="keys" label={keyText.title} keys={label(keysFor("keys"))} onAction={actions.keys} />
        {/* Closing the window only hides it; this ends the app and frees the shortcut. */}
        <MenuItem id="quit" label={pickerText.quitLabel} keys={label(keysFor("quit"))} onAction={actions.quit} />
      </MenuSection>
    </MoreMenu>
  );
}
