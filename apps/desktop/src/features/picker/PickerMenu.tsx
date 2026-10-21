import type { Density, LibraryStatus, LoginItem, ZoteroLibrary } from "../../api";
import { keyLabel, keysFor } from "../../keys";
import { LibraryDetail } from "../../LibraryStatusLine";
import { densityNames, keyText, loginItemText, pickerText } from "../../messages";
import { isMac } from "../../platform";
import { MenuItem, MoreMenu } from "./MoreMenu";

const label = (accelerator: string) => keyLabel(accelerator, isMac);

/** The More menu: the library and its Zotero scope, view settings, starting
 * at login, the keys overlay and Quit. */
export function PickerMenu({ onClose, library, libraryDetail, zotero, density, largerText, startAtLogin, actions }: {
  onClose: (refocus: boolean) => void;
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
    <MoreMenu onClose={onClose}>
      {/* Where the references come from and how fresh they are, read
          as the group's name when the menu is reached by keyboard. */}
      <div role="group" className="menu-library" aria-label={libraryDetail?.filter(Boolean).join(". ")}>
      {libraryDetail && <LibraryDetail lines={libraryDetail} />}
      <MenuItem label={pickerText.chooseLibrary} title={pickerText.chooseTitle} onSelect={actions.choose} />
      {!library?.zotero && (
        <MenuItem label={pickerText.useZotero} title={pickerText.useZoteroTitle} onSelect={actions.useZotero} />
      )}
      {zotero && (
        <div role="group" aria-label={pickerText.zoteroLibraries}>
          {zotero.offered.map((choice) => {
            const checked = zotero.isChosen(choice);
            const last = checked && zotero.chosenCount === 1;
            return (
              <MenuItem key={typeof choice === "object" ? choice.group : choice}
                label={zotero.name(choice)}
                title={last ? pickerText.keepOneLibrary : undefined} checked={checked} disabled={last}
                onSelect={() => zotero.toggle(choice)} />
            );
          })}
        </div>
      )}
      {library?.name && (
        <MenuItem label={pickerText.reload} title={pickerText.reloadTitle} onSelect={actions.reload} />
      )}
      </div>
      <MenuItem label={pickerText.density(densityNames[density])} title={pickerText.densityTitle} onSelect={actions.density} />
      <MenuItem label={pickerText.largerText} checked={largerText} onSelect={actions.largerText} />
      {startAtLogin && (
        <MenuItem label={loginItemText.label(startAtLogin.item.name)} title={loginItemText.title} checked={startAtLogin.item.enabled} onSelect={startAtLogin.toggle} />
      )}
      {/* Closing the window only hides it; this ends the app and frees the shortcut. */}
      <MenuItem label={keyText.title} keys={label(keysFor("keys"))} onSelect={actions.keys} />
      <MenuItem label={pickerText.quitLabel} keys={label(keysFor("quit"))} onSelect={actions.quit} />
    </MoreMenu>
  );
}
