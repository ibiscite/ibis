// What Enter would paste for the chosen references: the formatted or key preview,
// tagged with the choice, style and library it was made for.
import { useEffect, useMemo, useRef, useState } from "react";
import * as nativeApi from "../../api";
import type { FilePreview } from "../../bindings";
import type { LibraryStatus } from "../../api";
import { parse } from "../../format/markup";
import { outputName, type FormattedCitation, type OutputId } from "../../format/outputs";
import { pickerText } from "../../messages";
import { choiceKey, type CitationReference } from "./choice";

export function useCitationOutput({ chosen, library, libraryVersion, sessionId = 0, opened = sessionId, isCurrentSession, format, style, previewFileCitation, refreshSnapshot }: {
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
}) {
  // Tagged with the style, choice and library it was made for, so Enter never
  // copies a preview of an earlier choice.
  const [formatted, setFormatted] = useState<{ key: string; request: object; session: number; opened: number; preview?: FilePreview; result: (FormattedCitation & { fileTicket?: string; fileHash?: string }) | "failed" | "stale"; problem?: string } | null>(null);
  const stalePreview = useRef<string | null>(null);
  // A preview stopped under a live owner is asked for once more per opening.
  const [attempt, setAttempt] = useState(0);
  const askedAgain = useRef<string | null>(null);
  const currentOpening = useRef(opened);
  currentOpening.current = opened;
  const currentSession = useRef(sessionId);
  currentSession.current = sessionId;
  const chosenKey = choiceKey(chosen);
  const chosenRevision = chosen[0]?.revision;
  // A style the engine cannot format now is not previewed as anything else.
  const output = style ?? format;
  // Reading unchanged status must not retry the same stale citation forever.
  const retryKey = JSON.stringify([output, library?.revision, chosenKey]);
  const previewKey = `${output}\u0002${libraryVersion}\u0002${chosenKey}`;
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
  const previewProblem = shown === "failed" ? formatted?.problem ?? (style ? pickerText.previewStyleError(outputName(style)) : pickerText.previewError) : undefined;
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
    })), output, chosenRevision, abort.signal).then((result) => ({ markup: result.html ? parse(result.html) : [result.text], text: result.text, html: result.html ?? "", warnings: result.warnings, spans: result.spans, preview: result, fileTicket: result.ticket, fileHash: result.content_hash }));
    made
      .then((result) => { if (owns()) { askedAgain.current = null; setFormatted({ key, request, session: sessionId, opened: currentOpening.current, preview: result.preview, result: result.preview.problem ? "failed" : result, problem: result.preview.problem ?? undefined }); } })
      .catch((error) => { if (owns() && !nativeApi.isLibraryLoading(error)) {
        if (nativeApi.isCancelled(error)) {
          // Nothing failed and this owner still wants the citation. A second
          // stop waits for the next change or opening: no loop, no message.
          const again = `${sessionId}\u0002${key}`;
          if (askedAgain.current !== again) {
            askedAgain.current = again;
            setAttempt((value) => value + 1);
          }
          return;
        }
        const stale = nativeApi.isSnapshotChanged(error);
        const shared = nativeApi.sharedCitationKey(error);
        const failure = typeof error === "object" && error !== null ? error as { code?: unknown; message?: unknown } : null;
        const message = failure?.code === "failed" && typeof failure.message === "string" ? failure.message : undefined;
        setFormatted({ key, request, session: sessionId, opened: currentOpening.current, result: stale ? "stale" : "failed", problem: shared ? pickerText.sharedKey(shared) : message });
        if (stale) {
          stalePreview.current = retryKey;
          refreshSnapshot();
        }
      } });
    return () => { live = false; abort.abort(); };
  }, [previewKey, request, sessionId]);
  return { formatted, previewReceipt, chosenKey, shown, previewProblem, displayedOutput };
}
