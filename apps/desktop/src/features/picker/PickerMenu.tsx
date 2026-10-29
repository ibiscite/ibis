import type { Ref, RefObject } from "react";
import type { Density, LibraryStatus, LoginItem, ZoteroLibrary } from "../../api";
import { keyLabel, keysFor } from "../../keys";
import { LibraryDetail } from "../../LibraryStatusLine";
import { densityNames, keyText, loginItemText, pickerText } from "../../messages";
import { currentPlatform, isMac } from "../../platform";
import { MenuDetail } from "../../ui/Menu";
import { MenuItem, MenuSection, MenuSeparator, MoreMenu } from "./MoreMenu";

const label = (accelerator: string) => keyLabel(accelerator, isMac);

/** The More button and menu: the library and its Zotero scope, view settings,
 * starting at login, updates, the keys overlay and Quit. */
export function PickerMenu({ isOpen, onOpenChange, buttonRef, searchRef, library, libraryDetail, lastOutcome, zotero, density, largerText, searchMark, showPastePreview, pinned, startAtLogin, updates, actions }: {
  isOpen: boolean; onOpenChange: (open: boolean) => void;
  buttonRef: Ref<HTMLButtonElement>; searchRef: RefObject<HTMLElement | null>;
  library: LibraryStatus | null | undefined;
  /** Where the references come from and how fresh they are. */
  libraryDetail: [string, string | null] | null;
  lastOutcome?: string | null;
  /** The Zotero libraries to offer, when the library is Zotero and can be narrowed. */
  zotero: { offered: ZoteroLibrary[]; isChosen: (choice: ZoteroLibrary) => boolean; chosenCount: number; name: (choice: ZoteroLibrary) => string; toggle: (choice: ZoteroLibrary) => void } | null;
  density: Density;
  largerText: boolean;
  searchMark: boolean;
  showPastePreview: boolean;
  pinned: boolean;
  /** Offered when the system supports it. */
  startAtLogin: { item: LoginItem; toggle: () => void } | null;
  /** The tray's update controls, for builds that have updates. */
  updates: { automatic: boolean; available: boolean; toggle: () => void; check: () => void } | null;
  actions: { choose: () => void; useZotero: () => void; reload: () => void; density: () => void; largerText: () => void; searchMark: () => void; preview: () => void; pin: () => void; keys: () => void; settings: (() => void) | null;
    /** Offered when Ibis remembers formats for apps. */
    forgetApps?: (() => void) | null; quit: () => void };
}) {
  return (
    <MoreMenu isOpen={isOpen} onOpenChange={onOpenChange} buttonRef={buttonRef} searchRef={searchRef}
      keys={keysFor("more")}>
      {/* Where the references come from and how fresh they are, read
          as the group's name when the menu is reached by keyboard. */}
      <MenuSection id="library" label={[lastOutcome && pickerText.lastOutcome(lastOutcome), libraryDetail?.filter(Boolean).join(". ")].filter(Boolean).join(". ")}
        header={lastOutcome || libraryDetail ? <>{lastOutcome && <MenuDetail title={lastOutcome} />}{libraryDetail && <LibraryDetail lines={libraryDetail} />}</> : undefined}>
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
        {actions.forgetApps && <MenuItem id="forget-apps" label={pickerText.forgetApps} title={pickerText.forgetAppsTitle} onAction={actions.forgetApps} />}
      </MenuSection>
      <MenuSection id="view-toggles">
        <MenuItem id="pin" label={pinned ? pickerText.stopKeepingOpen : pickerText.keepOpenMenu} keys={label(keysFor("pin"))} checked={pinned} onAction={actions.pin} />
        <MenuItem id="paste-preview" label={pickerText.showPastePreview} keys={label(keysFor("preview"))} checked={showPastePreview} onAction={actions.preview} />
        <MenuItem id="larger-text" label={pickerText.largerText} checked={largerText} onAction={actions.largerText} />
        <MenuItem id="search-mark" label={pickerText.searchMark} checked={searchMark} onAction={actions.searchMark} />
        {startAtLogin && (
          <MenuItem id="start-at-login" label={loginItemText.label(currentPlatform())} title={loginItemText.title(currentPlatform())} checked={startAtLogin.item.enabled} onAction={startAtLogin.toggle} />
        )}
        {updates && (
          <MenuItem id="check-automatically" label={pickerText.checkAutomatically} checked={updates.automatic} onAction={updates.toggle} />
        )}
      </MenuSection>
      <MenuSeparator />
      <MenuSection id="ibis">
        {updates && <MenuItem id="check-for-updates" label={updates.available ? pickerText.updateAvailable : pickerText.checkForUpdates} onAction={updates.check} />}
        {actions.settings && <MenuItem id="settings" label={pickerText.settings} keys={label(keysFor("settings"))} onAction={actions.settings} />}
        <MenuItem id="keys" label={keyText.title} keys={label(keysFor("keys"))} onAction={actions.keys} />
        {/* Closing the window only hides it; this ends the app and frees the shortcut. */}
        <MenuItem id="quit" label={pickerText.quitLabel} keys={label(keysFor("quit"))} onAction={actions.quit} />
      </MenuSection>
    </MoreMenu>
  );
}
