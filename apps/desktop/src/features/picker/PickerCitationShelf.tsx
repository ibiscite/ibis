import type { RefObject } from "react";
import type { SearchResult } from "../../api";
import { pickerText } from "../../messages";
import { CitationShelf } from "./CitationShelf";
import type { CitationReference } from "./choice";
import { styleOf, type OutputId } from "../../format/outputs";
import type { usePickerSession } from "./usePickerSession";
import type { useCitationOutput } from "./useCitationOutput";
import type { useCitationRows } from "./useCitationRows";
import type { usePickerSource } from "./usePickerSource";
import type { useZoteroScope } from "./useZoteroScope";
import type { CitationInspectorState } from "./useCitationInspector";
const noSelection = new Map<string, CitationReference>();

export function PickerCitationShelf({
  state, send, showCitations, results, output, chosen, changeReference, copyChosen, libraryOf, format,
  input, sourceState, inspector, deliverable,
}: {
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
  sourceState: ReturnType<typeof usePickerSource>;
  inspector: CitationInspectorState;
  /** Enter would deliver the shown citation now, not wait for a search or a setting. */
  deliverable: boolean;
}) {
  const { selected, selectionChanged } = state;
  const { previewReceipt, displayedOutput, shown, previewProblem } = output;
  const { source, sourceAction } = sourceState;
  const style = styleOf(format);
  /** Names the library too when another chosen reference has the same key. */
  const removeLabel = (r: SearchResult) => {
    const library = libraryOf(r.source ?? "");
    const shared = [...selected.values()].some((other) => other.key === r.key && other.source !== r.source);
    return shared && library ? pickerText.removeReferenceIn(r.key, library) : pickerText.removeReference(r.key);
  };
  return <CitationShelf deliverable={deliverable} presentation={state.visibility} present={showCitations && results.length > 0} announce={selectionChanged} selected={showCitations ? selected : noSelection} removeLabel={removeLabel} onRemove={(r) => {
    send({ type: "remove-reference", reference: r });
    input.current?.focus({ preventScroll: true });
  }} highlightedReferences={selected.size === 0 ? chosen : undefined} previewReceipt={previewReceipt}
    onChangeReference={changeReference} onSubmit={copyChosen} copyProblem={state.copyError} inspector={inspector} referenceLibrary={(r) => libraryOf(r.source ?? "")} style={style} keys={style === null}
    shown={showCitations ? displayedOutput : undefined} outputWarnings={shown && typeof shown === "object" ? shown.warnings : []}
    problem={previewProblem} copied={showCitations ? state.shelfCopied : null} focusable={selected.size > 0} fallbackFocusRef={input}
    source={source.kind === "zotero" ? { ...sourceAction, label: pickerText.showInZotero } : sourceAction} onOpenSource={(reference) => sourceState.openReference(reference.id)} />;
}
