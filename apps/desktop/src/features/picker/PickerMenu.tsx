import type { Ref, RefObject } from "react";
import type { Density, ProjectStatus } from "../../api";
import { keyLabel, keysFor } from "../../keys";
import { densityNames, keyText, pickerText, sessionText } from "../../messages";
import { isMac } from "../../platform";
import { SourceIcon } from "../../ui/SourceAction";
import type { SourceActionProps } from "../../ui/SourceAction";
import { MenuDetail, MenuSubmenu } from "../../ui/Menu";
import { MenuItem, MenuSection, MenuSeparator, MoreMenu } from "./MoreMenu";

const label = (accelerator: string) => keyLabel(accelerator, isMac);

/** More holds working actions; preferences live in Settings and source choices
 * have their own library menu. Rows is the one look changed while working. */
export function PickerMenu({ isOpen, onOpenChange, buttonRef, searchRef, libraryDetail, lastOutcome, density, pinned, actions, source, project }: {
  source?: SourceActionProps;
  isOpen: boolean; onOpenChange: (open: boolean) => void;
  buttonRef: Ref<HTMLButtonElement>; searchRef: RefObject<HTMLElement | null>;
  /** Where the references come from and how fresh they are. */
  libraryDetail: [string, string | null] | null;
  lastOutcome?: string | null;
  density: Density;
  pinned: boolean;
  actions: { density: (density: Density) => void; pin: () => void; keys: () => void; settings: (() => void) | null;
    /** Offered once this app has a citation from this run of Ibis. */
    changeLast?: (() => void) | null;
    /** Offered for a proven ordinary app target. */
    copyOnlyHere?: { app: string; choose: () => void } | null;
    forgetRecent?: (() => void) | null; quit: () => void;
    sessionReferences?: (() => void) | null };
  /** The bibliography keys are checked against, and the entry of the highlighted row. */
  project?: { status: ProjectStatus | null; choose: (() => void) | null; stop: (() => void) | null; copyEntry: (() => void) | null };
}) {
  return (
    <MoreMenu isOpen={isOpen} onOpenChange={onOpenChange} buttonRef={buttonRef} searchRef={searchRef}
      keys={keysFor("more")}>
      {/* Where the references come from and how fresh they are, read
          as the group's name when the menu is reached by keyboard. */}
      <MenuSection id="library" label={[lastOutcome && pickerText.lastOutcome(lastOutcome), libraryDetail?.filter(Boolean).join(". ")].filter(Boolean).join(". ")}
        // The footer's status already names the library right under this menu, so More
        // shows it only in the section's name for screen readers, never a second time.
        header={lastOutcome ? <MenuDetail title={lastOutcome} /> : undefined}>
        {source && <MenuItem id="source" label={source.label} icon={<SourceIcon icon={source.icon} file={source.file} />} keys={source.keys ? label(source.keys) : undefined} onAction={source.open} />}
      </MenuSection>
      <MenuSeparator />
      {project?.choose && <MenuSection id="project" label={project.status ? pickerText.projectDetail(project.status) : undefined}
        header={project.status ? <MenuDetail title={pickerText.projectDetail(project.status)} /> : undefined}>
        {project.copyEntry && project.status && <MenuItem id="project-entry" label={pickerText.copyProjectEntry(project.status.name)} onAction={project.copyEntry} />}
        <MenuItem id="project-choose" label={project.status ? pickerText.chooseOtherProject : pickerText.chooseProject} onAction={project.choose} />
        {project.status && project.stop && <MenuItem id="project-stop" label={pickerText.stopProject(project.status.name)} onAction={project.stop} />}
      </MenuSection>}
      {project?.choose && <MenuSeparator />}
      {actions.changeLast && <MenuSection id="citation">
        <MenuItem id="change-last" label={pickerText.changeLastCitation} keys={label(keysFor("changeLast"))} onAction={actions.changeLast} />
      </MenuSection>}
      <MenuSection id="view">
        <MenuSubmenu id="density" label={pickerText.rows} value={pickerText.density(densityNames[density])} title={pickerText.densityTitle}>
          <MenuSection id="densities" selectionMode="single">
            {(["compact", "comfortable"] as const).map((choice) => <MenuItem key={choice} id={choice}
              label={densityNames[choice]} example={pickerText.densityExamples[choice]} checked={density === choice}
              closeOnSelect onAction={() => actions.density(choice)} />)}
          </MenuSection>
        </MenuSubmenu>
        {actions.copyOnlyHere && <MenuItem id="copy-only-here" label={pickerText.copyOnlyHere(actions.copyOnlyHere.app)} onAction={actions.copyOnlyHere.choose} />}
        {actions.sessionReferences && <MenuItem id="session-references" label={sessionText.menu} onAction={actions.sessionReferences} />}
        {actions.forgetRecent && <MenuItem id="forget-recent" label="Forget Recent Citations" onAction={actions.forgetRecent} />}
      </MenuSection>
      <MenuSection id="view-toggles">
        <MenuItem id="pin" label={pickerText.keepOpenMenu} keys={label(keysFor("pin"))} checked={pinned} onAction={actions.pin} />
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
