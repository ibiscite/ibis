import { useMemo, useRef, type ComponentProps, type RefObject } from "react";
import type { ResultList } from "./ResultList";
import type { PickerApi } from "../../App";
import type { Locator, SearchResult } from "../../api";
import type { usePointerSelection } from "../../pointerSelection";
import type { usePickerSession } from "./usePickerSession";
import type { useLibrarySearch } from "./useLibrarySearch";
import type { useOperations } from "./useOperations";
import { choiceKey, idKey, type CitationReference, type Choice } from "./choice";

export function useCitationRows({
  api, pickerSession, searchState, chosenKey, chosen, showCitations, format, snapshotRevision, input,
  pointer, requestCopy, report,
}: {
  api: PickerApi;
  pickerSession: ReturnType<typeof usePickerSession>;
  searchState: ReturnType<typeof useLibrarySearch>;
  chosenKey: string;
  chosen: CitationReference[];
  showCitations: boolean;
  format: Choice;
  snapshotRevision: string | undefined;
  input: RefObject<HTMLInputElement | null>;
  pointer: ReturnType<typeof usePointerSelection>;
  requestCopy: (choice: string | null) => void;
  report: ReturnType<typeof useOperations>["report"];
}) {
  const { session, state, send, cancelCopy, setActive } = pickerSession;
  const { results, ready, list, interpretation, queryKey, pending: searchPending } = searchState;
  const { active, selected } = state;
  const { rowContext } = api;
  const { resetPointer } = pointer;
  const highlightedChoice = choiceKey(results.slice(active, active + 1).map((r) => ({ ...r, locator: interpretation?.locator })));
  const latestHighlight = useRef(highlightedChoice);
  latestHighlight.current = highlightedChoice;
  const toggle = (result?: SearchResult) => send({ type: "toggle", result, locator: interpretation?.locator });
  const changeLocator = (reference: CitationReference, locator: Readonly<Locator> | null) => {
    send({ type: "reference", reference, change: { locator: locator ? { ...locator } : null }, selectedOnly: true });
    input.current?.focus({ preventScroll: true });
  };
  const changeReference = (reference: CitationReference, change: Pick<CitationReference, "locator" | "form">) => {
    send({ type: "reference", reference, change, initialSelection: list && ready ? results : undefined });
  };

  const rowScope = useMemo(() => ({ api, results, ready, selected, queryKey, format, snapshotRevision }), [api, results, ready, selected, queryKey, format, snapshotRevision]);
  const latestRowScope = useRef(rowScope);
  latestRowScope.current = rowScope;
  const contextRequest = useRef(0);

  const copyChosen = () => {
    if (chosen.length > 0) requestCopy(chosenKey);
    else if (showCitations && searchPending) requestCopy(null);
  };
  const onChoose: NonNullable<ComponentProps<typeof ResultList>["onChoose"]> = (i, r, action) => {
    // Rows of an earlier query, still shown while a newer search
    // is pending, are not acted on by a click or Shift+click (#507).
    if (action === "toggle") cancelCopy();
    if (!ready) return;
    pointer.choose(i);
    if (action === "toggle") toggle(r);
    else if (action === "copy") requestCopy(selected.size > 0 || list ? chosenKey : choiceKey([{ ...r, locator: interpretation?.locator }]));
    input.current?.focus();
  };
  const onContext: NonNullable<ComponentProps<typeof ResultList>["onContext"]> = (i, r) => {
    if (!ready || !rowContext) return;
    resetPointer();
    setActive(i);
    const scope = rowScope, visibility = session.getSnapshot().visibility, request = ++contextRequest.current;
    const owns = () => scope === latestRowScope.current && visibility === session.getSnapshot().visibility && request === contextRequest.current && latestHighlight.current === choiceKey([{ ...r, locator: interpretation?.locator }]);
    void rowContext(selected.has(idKey(r.id))).then((action) => {
      if (!owns()) return;
      report("rowContext", false);
      if (action === "toggle") toggle(r);
      else if (action === "copy") requestCopy(selected.size > 0 || list ? chosenKey : choiceKey([{ ...r, locator: interpretation?.locator }]));
      if (action !== null || document.hasFocus()) input.current?.focus({ preventScroll: true });
    }).catch(() => { if (owns()) { report("rowContext"); if (document.hasFocus()) input.current?.focus({ preventScroll: true }); } });
  };
  const onToggle: NonNullable<ComponentProps<typeof ResultList>["onToggle"]> = (i, r) => {
    cancelCopy();
    if (!ready) return;
    pointer.choose(i);
    toggle(r);
    input.current?.focus();
  };
  return { toggle, changeLocator, changeReference, copyChosen, onChoose, onContext, onToggle };
}
