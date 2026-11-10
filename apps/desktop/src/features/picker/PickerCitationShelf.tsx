import type { RefObject } from "react";
import type { CitationLineActions } from "./CitationShelf";
import type { SearchResult } from "../../api";
import { CitationShelf } from "./CitationShelf";
import type { CitationReference } from "./choice";
import { styleOf, type OutputId } from "../../format/outputs";
import type { usePickerSession } from "./usePickerSession";
import type { useCitationOutput } from "./useCitationOutput";
import type { useCitationRows } from "./useCitationRows";
import type { useZoteroScope } from "./useZoteroScope";
import type { CitationInspectorState } from "./useCitationInspector";
const noSelection = new Map<string, CitationReference>();

export function PickerCitationShelf({
  state, send, showCitations, results, output, chosen, changeReference, copyChosen, libraryOf, format,
  input, inspector, deliverable, citationLine,
}: {
  citationLine: RefObject<CitationLineActions | null>;
  state: ReturnType<typeof usePickerSession>["state"];
  send: ReturnType<typeof usePickerSession>["send"];
  showCitations: boolean;
  results: SearchResult[];
  output: ReturnType<typeof useCitationOutput>;
  chosen: CitationReference[];
  changeReference: ReturnType<typeof useCitationRows>["changeReference"];
  copyChosen: ReturnType<typeof useCitationRows>["copyChosen"];
  libraryOf: ReturnType<typeof useZoteroScope>["libraryOf"];
  format: OutputId;
  input: RefObject<HTMLInputElement | null>;
  inspector: CitationInspectorState;
  /** Enter would deliver the shown citation now, not wait for a search or a setting. */
  deliverable: boolean;
}) {
  const { selected, selectionChanged } = state;
  const { previewReceipt, displayedOutput, shown, previewProblem } = output;
  const style = styleOf(format);
  return <CitationShelf actionsRef={citationLine} keepOrder={state.keepOrder}
    onMoveReference={(reference, shown, step) => send({ type: "move-reference", reference, shown, step })}
    onKeepOrder={(value) => { send({ type: "keep-order", value }); input.current?.focus({ preventScroll: true }); }} deliverable={deliverable} presentation={state.visibility} present={showCitations && results.length > 0} announce={selectionChanged} selected={showCitations ? selected : noSelection} onRemove={(r) => {
    send({ type: "remove-reference", reference: r });
    input.current?.focus({ preventScroll: true });
  }} highlightedReferences={selected.size === 0 ? chosen : undefined} previewReceipt={previewReceipt}
    onChangeReference={changeReference} onSubmit={copyChosen} copyProblem={state.copyError} inspector={inspector} referenceLibrary={(r) => libraryOf(r.source ?? "")} style={style} keys={style === null}
    shown={showCitations ? displayedOutput : undefined} outputWarnings={shown && typeof shown === "object" ? shown.warnings : []}
    outputNotices={shown && typeof shown === "object" ? shown.notices : undefined}
    problem={previewProblem} copied={showCitations ? state.shelfCopied : null} focusable={selected.size > 0} fallbackFocusRef={input} />;
}
