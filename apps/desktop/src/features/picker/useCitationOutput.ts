// What Enter would paste for the chosen references: the formatted or key preview,
// tagged with the choice, style and library it was made for.
import { useEffect, useMemo, useRef, useState } from "react";
import * as nativeApi from "../../api";
import type { FilePreview } from "../../bindings";
import type { LibraryStatus } from "../../api";
import { parse } from "../../format/markup";
import { outputName, type FormattedCitation, type OutputId } from "../../format/outputs";
import { say } from "../../i18n";
import { choiceKey, type CitationReference } from "./choice";

export function useCitationOutput({ chosen, library, libraryVersion, sessionId = 0, opened = sessionId, isCurrentSession, format, style, previewFileCitation, refreshSnapshot, projectContext, contextReady = true }: {
  /** The selection, or the highlighted result when nothing is selected. */
  chosen: CitationReference[];
  library: LibraryStatus | null | undefined;
  libraryVersion: number;
  sessionId?: number;
  /** Advances before a reopened window's first frame; a failure belongs to one. */
  opened?: number;
  isCurrentSession?: (id: number) => boolean;
  format: OutputId;
  /** The format, when it is a style and formatting is available. */
  style: OutputId | null;
  previewFileCitation: typeof nativeApi.previewFileCitation;
  /** Reads the library status again when the preview meets a newer snapshot. */
  refreshSnapshot: () => void;
  projectContext?: [number, number] | null;
  /** False while the destination, and with it the project context, is being read. */
  contextReady?: boolean;
}) {
  // Tagged with the style, choice and library it was made for, so Enter never
  // copies a preview of an earlier choice.
  const [formatted, setFormatted] = useState<{ key: string; request: object; session: number; opened: number; preview?: FilePreview; result: (FormattedCitation & { fileTicket?: string; fileHash?: string }) | "failed" | "stale"; problem?: string } | null>(null);
  const stalePreview = useRef<string | null>(null);
  // A preview stopped under a live owner is asked for once more per opening.
  const [attempt, setAttempt] = useState(0);
  const askedAgain = useRef<string | null>(null);
  // A refusal under a context that is still being read waits for that answer.
  const waitingForContext = useRef<string | null>(null);
  const contextSettled = useRef(contextReady);
  contextSettled.current = contextReady;
  const currentOpening = useRef(opened);
  currentOpening.current = opened;
  const currentSession = useRef(sessionId);
  currentSession.current = sessionId;
  const chosenKey = choiceKey(chosen);
  const chosenRevision = chosen[0]?.revision;
  // A style the engine cannot format now is not previewed as anything else.
  const output = style ?? format;
  // Reading unchanged status must not retry the same stale citation forever.
  const retryKey = JSON.stringify([output, library?.revision, chosenKey, projectContext]);
  const previewKey = `${output}\u0002${libraryVersion}\u0002${chosenKey}\u0002${JSON.stringify(projectContext)}`;
  // A -> B -> A is a new operation: B has retired A's native copy ticket.
  // Equal input alone cannot revive that receipt before fresh A finishes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new attempt is a new request for the same key.
  const request = useMemo(() => ({ key: previewKey }), [previewKey, attempt]);
  const lastStarted = useRef<object | null>(null);
  const previousRequest = useRef({ sessionId, key: previewKey });
  // An empty search/reconciliation performs no native preview and cannot
  // retire its settled ticket. A -> B -> A still waits if B actually started.
  // A failure says nothing about the next try: choosing the citation again or
  // reopening prepares it afresh, and an earlier opening's failure is never shown.
  const failed = formatted?.result === "failed";
  const reusable = !failed && formatted?.key === previewKey && lastStarted.current === formatted.request;
  const ownsResult = (formatted?.request === request && !(failed && (formatted.session !== sessionId || formatted.opened !== opened))) || reusable;
  // What Enter would copy now; null while it is being formatted.
  const shown = ownsResult ? formatted?.result ?? null : null;
  // Why the preview failed, when the user can fix it.
  const previewProblem = shown === "failed" ? formatted?.problem ?? (style ? say("picker-preview-style-error", { style: outputName(style) }) : say("picker-preview-error")) : undefined;
  const previewReceipt = ownsResult ? formatted?.preview : undefined;
  const displayedOutput = chosen.length > 0 ? shown : undefined;
  // Format the preview again whenever the choice or the style changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: previewKey covers the choice, its locators, forms, revision and the style or key format, so it is the only trigger.
  useEffect(() => {
    const previous = previousRequest.current;
    previousRequest.current = { sessionId, key: previewKey };
    if (chosen.length === 0 || !chosenRevision) return;
    if (reusable && formatted && formatted.request !== request) {
      lastStarted.current = request;
      setFormatted({ ...formatted, request });
      return;
    }
    // A settled prepared result survives reopening, including permission
    // detours. Only unfinished or failed work needs a new session's request.
    if (previous.sessionId !== sessionId && previous.key === previewKey && formatted?.request === request && !failed) return;
    if (stalePreview.current === retryKey) {
      setFormatted({ key: previewKey, request, session: sessionId, opened, result: "stale" });
      return;
    }
    let live = true;
    const owns = () => live && currentSession.current === sessionId && (isCurrentSession?.(sessionId) ?? true);
    const abort = new AbortController();
    lastStarted.current = request;
    const key = previewKey;
    const made = previewFileCitation(chosen.map((r) => ({
      id: r.id, cite: r.cite ?? "",
      locator: r.locator?.input === "page-editor" ? null : r.locator ?? null,
      ...(r.locator?.input === "page-editor" ? { page_input: r.locator.value } : {}),
      ...(r.form ? { form: r.form } : {}),
    })), output, chosenRevision, abort.signal, projectContext).then((result) => ({ markup: result.html ? parse(result.html) : [result.text], text: result.text, html: result.html ?? "", warnings: result.warnings, spans: result.spans, project_refusal: result.project_refusal, notices: result.notices, preview: result, fileTicket: result.ticket, fileHash: result.content_hash }));
    made
      .then((result) => { if (owns()) { askedAgain.current = null; setFormatted({ key, request, session: sessionId, opened: currentOpening.current, preview: result.preview, result: result.preview.problem ? "failed" : result, problem: result.preview.problem ?? undefined }); } })
      .catch((error) => { if (owns() && !nativeApi.isLibraryLoading(error)) {
        if (nativeApi.isCancelled(error)) {
          // The engine also says "cancelled" when it refuses the project
          // context it was asked under, so this is not always newer work.
          // While that context is being read, its answer asks again. Otherwise
          // ask once more; a second refusal is stated, never Formatting… forever.
          const again = `${sessionId}\u0002${key}`;
          if (!contextSettled.current) {
            waitingForContext.current = again;
            return;
          }
          if (askedAgain.current !== again) {
            askedAgain.current = again;
            setAttempt((value) => value + 1);
            return;
          }
          setFormatted({ key, request, session: sessionId, opened: currentOpening.current, result: "failed", problem: say("picker-preview-refused") });
          return;
        }
        const stale = nativeApi.isSnapshotChanged(error);
        const shared = nativeApi.sharedCitationKey(error);
        const failure = typeof error === "object" && error !== null ? error as { code?: unknown; message?: unknown } : null;
        const message = failure?.code === "failed" && typeof failure.message === "string" ? failure.message : undefined;
        setFormatted({ key, request, session: sessionId, opened: currentOpening.current, result: stale ? "stale" : "failed", problem: shared ? say("picker-shared-key", { key: shared }) : message });
        if (stale) {
          stalePreview.current = retryKey;
          refreshSnapshot();
        }
      } });
    return () => { live = false; abort.abort(); };
  }, [previewKey, request, sessionId]);
  // The context's answer may keep the same key, so the refused citation is asked again.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the context's answer is the only trigger; the key is compared, not watched.
  useEffect(() => {
    const waited = waitingForContext.current;
    if (!contextReady || waited === null) return;
    waitingForContext.current = null;
    if (waited !== `${currentSession.current}\u0002${previewKey}`) return;
    askedAgain.current = null;
    setAttempt((value) => value + 1);
  }, [contextReady]);
  return { formatted, previewReceipt, chosenKey, shown, previewProblem, displayedOutput };
}
