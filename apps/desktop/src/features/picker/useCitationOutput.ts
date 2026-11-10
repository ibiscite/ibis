// What Enter would paste for the chosen references: the formatted or key preview,
// tagged with the choice, style and library it was made for. The picker session
// owns the preview (decision 0035); this hook tells it what to prepare and
// reads its answer.
import { useEffect, useLayoutEffect } from "react";
import type * as nativeApi from "../../api";
import type { LibraryStatus } from "../../api";
import { outputName, type OutputId } from "../../format/outputs";
import { say } from "../../i18n";
import { choiceKey, type CitationReference } from "./choice";
import { ownedPreview, previewKey, type PreviewInputs } from "./preview";
import type { SessionState } from "./session";
import type { createSessionController } from "./sessionController";

export function useCitationOutput({ session, state, chosen, library, libraryVersion, format, style, previewFileCitation, projectContext, contextReady = true }: {
  session: ReturnType<typeof createSessionController>;
  state: SessionState;
  /** The selection, or the highlighted result when nothing is selected. */
  chosen: CitationReference[];
  library: LibraryStatus | null | undefined;
  libraryVersion: number;
  format: OutputId;
  /** The format, when it is a style and formatting is available. */
  style: OutputId | null;
  previewFileCitation: typeof nativeApi.previewFileCitation;
  projectContext?: [number, number] | null;
  /** False while the destination, and with it the project context, is being read. */
  contextReady?: boolean;
}) {
  const chosenKey = choiceKey(chosen);
  const chosenRevision = chosen[0]?.revision;
  // A style the engine cannot format now is not previewed as anything else.
  const output = style ?? format;
  // Reading unchanged status must not retry the same stale citation forever.
  const retryKey = JSON.stringify([output, library?.revision, chosenKey, projectContext, state.keepOrder]);
  const key = previewKey(output, libraryVersion, chosenKey, projectContext, state.keepOrder);
  const inputs: PreviewInputs = {
    key, retryKey, contextReady,
    ask: chosen.length === 0 || !chosenRevision ? null : {
      output, revision: chosenRevision, projectContext, keepOrder: state.keepOrder,
      selection: chosen.map((r) => ({
        id: r.id, cite: r.cite ?? "",
        locator: r.locator?.input === "editor" ? null : r.locator ?? null,
        ...(r.locator?.input === "editor" ? { locator_input: { label: r.locator.label, text: r.locator.value } } : {}),
        ...(r.form ? { form: r.form } : {}),
        ...(r.prefix ? { prefix: r.prefix } : {}), ...(r.suffix ? { suffix: r.suffix } : {}),
      })),
    },
  };
  useLayoutEffect(() => { session.connect({ previewFileCitation }); });
  useEffect(() => () => session.stop(), [session]);
  // The key covers the choice, its fields and order, revision, the output and the
  // project context; the opening and the context's answer complete the request.
  // biome-ignore lint/correctness/useExhaustiveDependencies: those three are the only triggers.
  useLayoutEffect(() => { session.dispatch({ type: "preview-inputs", inputs }); }, [key, state.session, contextReady]);

  // The session hears of a new key after this render: until then its answer for
  // the earlier key is not this choice's.
  const result = ownedPreview(state.preview, key, state.session, state.opened);
  // What Enter would copy now; null while it is being formatted.
  const shown = result ? result.value : null;
  // Why the preview failed, when the person can fix it.
  const previewProblem = shown === "failed" ? result?.problem ?? (style ? say("picker-preview-style-error", { style: outputName(style) }) : say("picker-preview-error")) : undefined;
  const previewReceipt = result?.preview;
  const displayedOutput = chosen.length > 0 ? shown : undefined;
  return { formatted: result, previewReceipt, chosenKey, shown, previewProblem, displayedOutput };
}
