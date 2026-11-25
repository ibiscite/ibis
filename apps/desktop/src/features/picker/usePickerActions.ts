import type * as nativeApi from "../../api";
import { KEY_TABLE, type KeyBinding } from "../../keys";
import { flushSync } from "react-dom";
import { say } from "../../i18n";
import { useLayer, useNearestLayer } from "../../ui/layers";
import type { SearchResult } from "../../api";
import { choiceKey, idKey, type CitationReference } from "./choice";
import { citationDelivery, enterAction as describeEnter } from "./enterLabel";
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
  api, pickerSession, menus, libraryState, searchState, destination, shown, offered, canPaste, shortcut,
  perform, dismissChooser, toggle, writesStyle, practising, deciding, refusal,
}: {
  api: typeof nativeApi;
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
  dismissChooser: () => void;
  toggle: (result?: SearchResult) => void;
  /** The output being written is a style, which may sort. */
  writesStyle: boolean;
  /** Enter's next paste is the first screen's practice, into Ibis itself. */
  practising: boolean;
  /** Whether that is still being decided for this opening: Enter waits on it. */
  deciding: boolean;
  /** Why the shown citation is refused, when it is. */
  refusal: string | null;
}) {
  const { session, state, send, cancelCopy } = pickerSession;
  const { pinned, focusHome, leaveOverlays } = menus;
  const { choosingLibrary, welcomeShown, showCitations } = libraryState;
  const { ready, results, interpretation } = searchState;
  const { query, active, selected, interpretLocators } = state;
  const { format } = destination;
  const { hidePicker } = api;
  // The bottom of the layer stack; overlays register themselves above these.
  // Esc retains the chosen references at every step.
  useLayer("chooser", choosingLibrary, () => {
    cancelCopy();
    flushSync(dismissChooser);
    focusHome();
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
  const delivery = citationDelivery(destination.app, canPaste, stayOpen, destination.editorFormat);
  // The keys sheet names what Enter does from the same answer as the footer,
  // and lists the global shortcut too, when it works.
  const enterRow = destination.findOpen ? "shortcuts-action-find-open" as const : delivery.editorFormat ? "shortcuts-action-send" as const : delivery.pastes ? "shortcuts-action-paste" as const : "shortcuts-action-copy-only" as const;
  const bindings: readonly KeyBinding[] = [
    ...KEY_TABLE.map((binding) => binding.action === "copy" ? { ...binding, label: enterRow } : binding),
    ...destination.findOpen ? [{ region: "search", action: "copyCitation", keys: ["Alt+Enter"] } as const] : [],
    ...shortcut?.registered ? [{ region: "system", action: "open", keys: [shortcut.shortcut], displayLabel: shortcut.desktop ? shortcut.shortcut : undefined } as const] : [],
  ];
  // What Enter does now, from the same state the copy uses (enterLabel.ts): one
  // outcome, named once by the bar's action; null when it does nothing.
  const enter = describeEnter({
    count: offered.length, preview: shown === "stale" || shown === "failed" ? shown : "ready",
    editor: Boolean(delivery.editorFormat), findOpen: destination.findOpen, practising, deciding, changing: state.changing !== null,
    format, app: destination.app, canPaste, stayOpen, ownOrder: state.keepOrder && writesStyle, refusal,
  });
  // Only a row that answers the current query can be added or given a locator.
  const highlighted = ready ? results[active] : undefined;
  // What the engine read around the search: a page off its end, a signal off its start.
  const usingLocator = ready && interpretLocators && showCitations && interpretation && (interpretation.locator || interpretation.prefix) ? interpretation : null;
  const prepared = highlighted && selected.get(idKey(highlighted.id));
  const canApplyLocator = usingLocator && prepared && prepared.revision === highlighted.revision &&
    choiceKey([prepared]) !== choiceKey([{ ...prepared, locator: usingLocator.locator ?? prepared.locator, prefix: usingLocator.prefix ?? prepared.prefix }]);
  const shiftEnterAction = highlighted && showCitations
    ? (selected.has(idKey(highlighted.id)) ? say("key-remove-from-citation") : say("key-add-to-citation")) : null;
  const escapeAction = nearest === "sheet" || nearest === "panel" || nearest === "chooser" ? say("key-close") : nearest === "menu" || nearest === "palette" ? say("key-close-menu")
    : nearest === "search" ? say("key-clear-search") : say("key-hide");
  const rememberedCitation = showCitations && destination.ready && state.view?.remembersLast && destination.appId ? state.lastCitations.get(destination.appId) : undefined;
  const lastCitation = !destination.editorFormat && rememberedCitation && (rememberedCitation.project ?? null) === (destination.writing?.id ?? null)
    && (!destination.writing || rememberedCitation.format === format) ? rememberedCitation : undefined;
  /** Change Last Citation acts on the shelf, so nothing stays in its way. */
  const changeLast = () => {
    if (!lastCitation) return;
    leaveOverlays();
    if (lastCitation.format !== format) destination.chooseOnce(lastCitation.format);
    send({ type: "change-last" });
  };
  /** Paste Reference List: offered once this app has citations from this run of
   * Ibis, in a style (a key format's list is the document's own bibliography). */
  const referenceList = showCitations && writesStyle && destination.citedHere > 0 ? {
    label: say(canPaste ? "picker-paste-reference-list" : "picker-copy-reference-list"),
    help: say("picker-reference-list-help", { count: destination.citedHere, app: destination.app ?? "" }),
    run: () => {
      leaveOverlays();
      send({ type: "reference-list" });
    },
  } : null;
  const toggleHighlighted = () => {
    toggle(highlighted);
    focusHome();
  };
  // The keys sheet lists Change Last Citation only while there is one to change, as More does.
  // Keys outputs always keep the order, so only a style offers the choice.
  const keepOrder = writesStyle ? { checked: state.keepOrder, toggle: () => send({ type: "keep-order", value: !state.keepOrder }) } : null;
  return { delivery, canChooseFormat: destination.canChooseFormat, keepOrder, bindings: bindings.filter((binding) => (lastCitation || binding.action !== "changeLast") && (destination.canChooseFormat || binding.action !== "format")), usingLocator, prepared, canApplyLocator, toggleHighlighted, canChangeLast: Boolean(lastCitation), changeLast, referenceList, enterOff: enter.off, enterHidden: enter.hidden, hints: { enter: enter.label, shiftEnter: shiftEnterAction, escape: escapeAction } };
}
