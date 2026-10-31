import type { Ref, RefObject } from "react";
import type { Density } from "../../api";
import { keyLabel, keysFor } from "../../keys";
import { LibraryDetail } from "../../LibraryStatusLine";
import { densityNames, keyText, pickerText } from "../../messages";
import { isMac } from "../../platform";
import { SourceIcon } from "../../ui/SourceAction";
import type { SourceActionProps } from "../../ui/SourceAction";
import { MenuDetail, MenuSubmenu } from "../../ui/Menu";
import { MenuItem, MenuSection, MenuSeparator, MoreMenu } from "./MoreMenu";

const label = (accelerator: string) => keyLabel(accelerator, isMac);

/** More holds working actions; preferences live in Settings and source choices
 * have their own library menu. Rows is the one look changed while working. */
export function PickerMenu({ isOpen, onOpenChange, buttonRef, searchRef, libraryDetail, lastOutcome, density, showPastePreview, pinned, actions, source }: {
  source?: SourceActionProps;
  isOpen: boolean; onOpenChange: (open: boolean) => void;
  buttonRef: Ref<HTMLButtonElement>; searchRef: RefObject<HTMLElement | null>;
  /** Where the references come from and how fresh they are. */
  libraryDetail: [string, string | null] | null;
  lastOutcome?: string | null;
  density: Density;
  showPastePreview: boolean;
  pinned: boolean;
  actions: { density: (density: Density) => void; preview: () => void; pin: () => void; keys: () => void; settings: (() => void) | null; quit: () => void };
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
        <MenuSubmenu id="density" label={pickerText.rows} value={pickerText.density(densityNames[density])} title={pickerText.densityTitle}>
          <MenuSection id="densities" selectionMode="single">
            {(["compact", "comfortable", "spacious"] as const).map((choice) => <MenuItem key={choice} id={choice}
              label={densityNames[choice]} example={pickerText.densityExamples[choice]} checked={density === choice}
              closeOnSelect onAction={() => actions.density(choice)} />)}
          </MenuSection>
        </MenuSubmenu>
      </MenuSection>
      <MenuSection id="view-toggles">
        <MenuItem id="pin" label={pinned ? pickerText.stopKeepingOpen : pickerText.keepOpenMenu} keys={label(keysFor("pin"))} checked={pinned} onAction={actions.pin} />
        <MenuItem id="paste-preview" label={pickerText.showPastePreview} keys={label(keysFor("preview"))} checked={showPastePreview} onAction={actions.preview} />
      </MenuSection>
      <MenuSeparator />
      <MenuSection id="ibis">
        <MenuItem id="keys" label={keyText.title} keys={label(keysFor("keys"))} onAction={actions.keys} />
        {actions.settings && <MenuItem id="settings" label={pickerText.settings} keys={label(keysFor("settings"))} onAction={actions.settings} />}
        {/* Closing the window only hides it; this ends the app and frees the shortcut. */}
        <MenuItem id="quit" label={pickerText.quitLabel} keys={label(keysFor("quit"))} onAction={actions.quit} />
      </MenuSection>
    </MoreMenu>
  );
}
