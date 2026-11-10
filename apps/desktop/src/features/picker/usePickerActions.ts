import { KEY_TABLE, type KeyBinding } from "../../keys";
import { say } from "../../i18n";
import { useLayer, useNearestLayer } from "../../ui/layers";
import type { SearchResult } from "../../api";
import type { PickerApi } from "../../App";
import { choiceKey, idKey, type CitationReference } from "./choice";
import { outputName } from "../../format/outputs";
import { enterLabel, enterPastes } from "./enterLabel";
import { closestMatchWarnings } from "./rows";
import type { usePickerSession } from "./usePickerSession";
import type { usePickerMenus } from "./usePickerMenus";
import type { usePickerLibrary } from "./usePickerLibrary";
import type { useLibrarySearch } from "./useLibrarySearch";
import type { useDestination } from "./useDestination";
import type { useCitationOutput } from "./useCitationOutput";
import type { useShortcutStatus } from "./useShortcutStatus";
import type { useOperations } from "./useOperations";
import type { CitationInspectorState } from "./useCitationInspector";

export function usePickerActions({
  api, pickerSession, menus, libraryState, searchState, destination, shown, inspector, offered, canPaste, shortcut,
  perform, dismissChooser, toggle, writesStyle,
}: {
  api: PickerApi;
  pickerSession: ReturnType<typeof usePickerSession>;
  menus: ReturnType<typeof usePickerMenus>;
  libraryState: Pick<ReturnType<typeof usePickerLibrary>, "choosingLibrary" | "welcomeShown" | "showCitations">;
  searchState: ReturnType<typeof useLibrarySearch>;
  destination: ReturnType<typeof useDestination>;
  shown: ReturnType<typeof useCitationOutput>["shown"];
  /** The citation on screen that Enter copies; it stays while a newer search runs, and Enter waits for that search. */
  offered: CitationReference[];
  canPaste: boolean;
  shortcut: ReturnType<typeof useShortcutStatus>["shortcut"];
  perform: ReturnType<typeof useOperations>["perform"];
  inspector: CitationInspectorState;
  dismissChooser: () => void;
  toggle: (result?: SearchResult) => void;
  /** The output being written is a style, which may sort. */
  writesStyle: boolean;
}) {
  const { session, state, send, cancelCopy } = pickerSession;
  const { pinned, focusHome, leaveOverlays } = menus;
  const { choosingLibrary, welcomeShown, showCitations } = libraryState;
  const { ready, results, list, interpretation } = searchState;
  const { query, active, selected, interpretLocators } = state;
  const { format } = destination;
  const { hidePicker } = api;
  // The bottom of the layer stack; overlays register themselves above these.
  // Esc retains the chosen references at every step.
  useLayer("panel", inspector.panel !== null, () => {
    cancelCopy();
    inspector.close();
  });
  useLayer("chooser", choosingLibrary, () => {
    cancelCopy();
    dismissChooser();
  });
  useLayer("search", Boolean(query) && !welcomeShown, () => {
    cancelCopy();
    send({ type: "escape-search" });
    focusHome();
  });
  useLayer("window", true, () => {
    cancelCopy();
    const presentation = session.getSnapshot().presentation;
    send({ type: "hiding" });
    void perform("hide", () => hidePicker(presentation));
  });
  const nearest = useNearestLayer();
  // A kept-open window or a citation with warnings stays open, so Enter copies.
  const stayOpen = pinned || Boolean(shown && typeof shown === "object" && (shown.warnings.length || closestMatchWarnings(offered).length));
  // The keys sheet names what Enter does from the same answer as the footer,
  // and lists the global shortcut too, when it works.
  const enterRow = enterPastes(destination.app, canPaste, stayOpen) ? "shortcuts-action-paste" as const : "shortcuts-action-copy-only" as const;
  const bindings: readonly KeyBinding[] = [
    ...KEY_TABLE.map((binding) => binding.action === "copy" ? { ...binding, label: enterRow } : binding),
    ...shortcut?.registered ? [{ region: "system", action: "open", keys: [shortcut.shortcut], displayLabel: shortcut.desktop ? shortcut.shortcut : undefined } as const] : [],
  ];
  // What Enter does now, from the same state the copy uses; null when it does nothing.
  // A refused or stale engine preview cannot be copied.
  const enterAction = offered.length === 0 || shown === "stale" || shown === "failed" ? null
    : state.changing !== null ? say("picker-change-citation", { format: outputName(format) })
    : enterLabel(format, offered.length, destination.app, canPaste, stayOpen, Boolean(list && selected.size === 0))
      // A style that would sort keeps the person's order: the footer says so, not the line.
      + (state.keepOrder && writesStyle && offered.length > 1 ? ` · ${say("picker-your-order")}` : "");
  // Only a row that answers the current query can be added or given a locator.
  const highlighted = ready ? results[active] : undefined;
  // What the engine read around the search: a page off its end, a signal off its start.
  const usingLocator = ready && interpretLocators && showCitations && interpretation && (interpretation.locator || interpretation.prefix) ? interpretation : null;
  const prepared = highlighted && selected.get(idKey(highlighted.id));
  const canApplyLocator = usingLocator && prepared && prepared.revision === highlighted.revision &&
    choiceKey([prepared]) !== choiceKey([{ ...prepared, locator: usingLocator.locator ?? prepared.locator, prefix: usingLocator.prefix ?? prepared.prefix }]);
  const shiftEnterAction = highlighted && showCitations
    ? (selected.has(idKey(highlighted.id)) ? say("key-remove-from-citation") : say("key-add-to-citation")) : null;
  const escapeAction = nearest === "sheet" || nearest === "panel" || nearest === "chooser" ? say("key-close") : nearest === "menu" ? say("key-close-menu")
    : nearest === "search" ? say("key-clear-search") : say("key-hide");
  const rememberedCitation = showCitations && destination.ready && state.view?.remembersLast && destination.appId ? state.lastCitations.get(destination.appId) : undefined;
  const lastCitation = rememberedCitation && (rememberedCitation.project ?? null) === (destination.writingProject?.id ?? null)
    && (!destination.writingProject || rememberedCitation.format === format) ? rememberedCitation : undefined;
  /** Change Last Citation acts on the shelf, so nothing stays in its way. */
  const changeLast = () => {
    if (!lastCitation) return;
    leaveOverlays();
    if (lastCitation.format !== format) destination.chooseOnce(lastCitation.format);
    send({ type: "change-last" });
  };
  const toggleHighlighted = () => {
    toggle(highlighted);
    focusHome();
  };
  // The keys sheet lists Change Last Citation only while there is one to change, as More does.
  // Keys outputs always keep the order, so only a style offers the choice.
  const keepOrder = writesStyle ? { checked: state.keepOrder, toggle: () => send({ type: "keep-order", value: !state.keepOrder }) } : null;
  return { keepOrder, bindings: lastCitation ? bindings : bindings.filter((binding) => binding.action !== "changeLast"), usingLocator, prepared, canApplyLocator, toggleHighlighted, canChangeLast: Boolean(lastCitation), changeLast, hints: { enter: enterAction, shiftEnter: shiftEnterAction, escape: escapeAction } };
}
