import { KEY_TABLE, type KeyBinding } from "../../keys";
import { keyText } from "../../messages";
import type { SearchResult } from "../../api";
import type { PickerApi } from "../../App";
import { choiceKey, idKey, type CitationReference } from "./choice";
import { enterLabel } from "./enterLabel";
import { closestMatchWarnings } from "./rows";
import type { usePickerSession } from "./usePickerSession";
import type { usePickerMenus } from "./usePickerMenus";
import type { usePickerLibrary } from "./usePickerLibrary";
import type { useLibrarySearch } from "./useLibrarySearch";
import type { useDestination } from "./useDestination";
import type { useCitationOutput } from "./useCitationOutput";
import type { useShortcutStatus } from "./useShortcutStatus";
import type { useOperations } from "./useOperations";

export function usePickerActions({
  api, pickerSession, menus, libraryState, searchState, destination, shown, chosen, canPaste, shortcut,
  perform, dismissChooser, toggle,
}: {
  api: PickerApi;
  pickerSession: ReturnType<typeof usePickerSession>;
  menus: ReturnType<typeof usePickerMenus>;
  libraryState: Pick<ReturnType<typeof usePickerLibrary>, "choosingLibrary" | "welcomeShown" | "showCitations">;
  searchState: ReturnType<typeof useLibrarySearch>;
  destination: ReturnType<typeof useDestination>;
  shown: ReturnType<typeof useCitationOutput>["shown"];
  chosen: CitationReference[];
  canPaste: boolean;
  shortcut: ReturnType<typeof useShortcutStatus>["shortcut"];
  perform: ReturnType<typeof useOperations>["perform"];
  dismissChooser: () => void;
  toggle: (result?: SearchResult) => void;
}) {
  const { session, state, send, cancelCopy } = pickerSession;
  const { pinned, input, keysOpen, menuOpen, closeKeys, closeMenu } = menus;
  const { choosingLibrary, welcomeShown, showCitations } = libraryState;
  const { current, results, list, interpretation } = searchState;
  const { query, active, selected, interpretLocators } = state;
  const { format } = destination;
  const { hidePicker } = api;
  const hide = () => {
    const presentation = session.getSnapshot().presentation;
    send({ type: "hiding" });
    void perform("hide", () => hidePicker(presentation));
  };
  // The keys overlay lists the global shortcut too, when it works.
  const bindings: readonly KeyBinding[] = shortcut?.registered
    ? [...KEY_TABLE, { region: "system", action: "open", keys: [shortcut.shortcut], displayLabel: shortcut.desktop ? shortcut.shortcut : undefined }]
    : KEY_TABLE;
  // What Enter does now, from the same state the copy uses; null when it does nothing.
  // A refused or stale engine preview cannot be copied.
  const enterAction = chosen.length === 0 || shown === "stale" || shown === "failed" ? null
    : enterLabel(format, chosen.length, destination.app, canPaste,
      pinned || Boolean(shown && typeof shown === "object" && (shown.warnings.length || closestMatchWarnings(chosen).length)), Boolean(list && selected.size === 0));
  const highlighted = current[active];
  const usingLocator = current === results && interpretLocators && showCitations ? interpretation?.locator : null;
  const prepared = highlighted && selected.get(idKey(highlighted.id));
  const canApplyLocator = usingLocator && prepared && prepared.revision === highlighted.revision &&
    choiceKey([prepared]) !== choiceKey([{ ...highlighted, locator: usingLocator, form: prepared.form }]);
  const shiftEnterAction = highlighted && showCitations
    ? (selected.has(idKey(highlighted.id)) ? keyText.removeFromCitation : keyText.addToCitation) : null;
  const escapeAction = keysOpen ? keyText.close : menuOpen ? keyText.closeMenu : query && !welcomeShown ? keyText.clearSearch : keyText.hide;
  const escapeStep = () => {
    cancelCopy();
    // Escape steps back one layer, retaining the chosen references.
    if (keysOpen) closeKeys();
    else if (menuOpen) closeMenu(true);
    else if (choosingLibrary) dismissChooser();
    else if (query && !welcomeShown) {
      send({ type: "escape-search" });
      input.current?.focus();
    } else hide();
  };
  const toggleHighlighted = () => {
    toggle(highlighted);
    input.current?.focus();
  };
  return { bindings, usingLocator, prepared, canApplyLocator, toggleHighlighted, escapeStep, hints: { enter: enterAction, shiftEnter: shiftEnterAction, escape: escapeAction } };
}
