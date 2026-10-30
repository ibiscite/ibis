import type { Ref, RefObject } from "react";
import type { Density, LoginItem } from "../../api";
import { keyLabel, keysFor } from "../../keys";
import { LibraryDetail } from "../../LibraryStatusLine";
import { densityNames, keyText, loginItemText, pickerText } from "../../messages";
import { currentPlatform, isMac } from "../../platform";
import { SourceIcon } from "../../ui/SourceAction";
import type { SourceActionProps } from "../../ui/SourceAction";
import { MenuDetail } from "../../ui/Menu";
import { MenuItem, MenuSection, MenuSeparator, MoreMenu } from "./MoreMenu";

const label = (accelerator: string) => keyLabel(accelerator, isMac);

/** More: the source action and library detail, view settings, starting at
 * login, updates, the keys overlay and Quit. Library choices have their own menu. */
export function PickerMenu({ isOpen, onOpenChange, buttonRef, searchRef, libraryDetail, lastOutcome, density, largerText, searchMark, showPastePreview, pinned, startAtLogin, updates, actions, source }: {
  source?: SourceActionProps;
  isOpen: boolean; onOpenChange: (open: boolean) => void;
  buttonRef: Ref<HTMLButtonElement>; searchRef: RefObject<HTMLElement | null>;
  /** Where the references come from and how fresh they are. */
  libraryDetail: [string, string | null] | null;
  lastOutcome?: string | null;
  density: Density;
  largerText: boolean;
  searchMark: boolean;
  showPastePreview: boolean;
  pinned: boolean;
  /** Offered when the system supports it. */
  startAtLogin: { item: LoginItem; toggle: () => void } | null;
  /** The tray's update controls, for builds that have updates. */
  updates: { automatic: boolean; available: boolean; toggle: () => void; check: () => void } | null;
  actions: { density: () => void; largerText: () => void; searchMark: () => void; preview: () => void; pin: () => void; keys: () => void; settings: (() => void) | null;
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
        {source && <MenuItem id="source" label={source.label} icon={<SourceIcon icon={source.icon} file={source.file} />} keys={source.keys ? label(source.keys) : undefined} onAction={source.open} />}
      </MenuSection>
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
