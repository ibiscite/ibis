// What Enter would paste for the chosen references: the formatted or key preview,
// tagged with the choice, style and library it was made for.
import { useEffect, useRef, useState } from "react";
import * as nativeApi from "../../api";
import type { CopyFormat, LibraryStatus } from "../../api";
import type { FormattedCitation } from "../../format/styles";
import { parse } from "../../format/markup";
import type { StyleId } from "../../format/styles";
import { pickerText } from "../../messages";
import { choiceKey, styleOf, type Choice, type CitationReference } from "./choice";

export function useCitationOutput({ chosen, library, libraryVersion, format, style, showPastePreview, previewFileCitation, refreshSnapshot }: {
  /** The selection, or the highlighted result when nothing is selected. */
  chosen: CitationReference[];
  library: LibraryStatus | null | undefined;
  libraryVersion: number;
  format: Choice;
  /** The citation style, when one is chosen and formatting is available. */
  style: StyleId | null;
  showPastePreview: boolean;
  previewFileCitation: typeof nativeApi.previewFileCitation;
  /** Reads the library status again when the preview meets a newer snapshot. */
  refreshSnapshot: () => void;
}) {
  // Tagged with the style, choice and library it was made for, so Enter never
  // copies a preview of an earlier choice.
  const [formatted, setFormatted] = useState<{ key: string; result: (FormattedCitation & { fileTicket?: string; fileHash?: string }) | "failed" | "stale"; problem?: string } | null>(null);
  const stalePreview = useRef<string | null>(null);
  const chosenKey = choiceKey(chosen);
  const chosenRevision = chosen[0]?.revision;
  const keyFormat = styleOf(format) === null ? (format as CopyFormat) : null;
  const output = style ?? format;
  // Reading unchanged status must not retry the same stale citation forever.
  const retryKey = JSON.stringify([output, library?.revision, chosenKey]);
  const previewKey = `${output}\u0002${libraryVersion}\u0002${chosenKey}`;
  // What Enter would copy now; null while it is being formatted.
  const shown = formatted?.key === previewKey ? formatted.result : null;
  // Why the preview failed, when the user can fix it.
  const previewProblem = shown === "failed" ? formatted?.problem : undefined;
  // Hiding citation text never hides the reason it cannot be copied.
  const displayedOutput = chosen.length > 0 && (showPastePreview || shown === "failed") ? shown : undefined;
  // Format the preview again whenever the choice or the style changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: previewKey covers the choice, its locators, revision and the style or key format, so it is the only trigger.
  useEffect(() => {
    if (chosen.length === 0 || !chosenRevision) return;
    if (stalePreview.current === retryKey) {
      setFormatted({ key: previewKey, result: "stale" });
      return;
    }
    let live = true;
    const key = previewKey;
    const made = previewFileCitation(chosen.map((r) => ({ id: r.id, cite: r.cite ?? "", locator: r.locator ?? null, ...(r.form ? { form: r.form } : {}) })), keyFormat ?? "pandoc", style, chosenRevision).then((result) => ({ markup: result.html ? parse(result.html) : [result.text], text: result.text, html: result.html ?? "", warnings: result.warnings, fileTicket: result.ticket, fileHash: result.content_hash }));
    made
      .then((result) => { if (live) setFormatted({ key, result }); })
      .catch((error) => { if (live) {
        const stale = nativeApi.isSnapshotChanged(error);
        const shared = nativeApi.sharedCitationKey(error);
        const failure = typeof error === "object" && error !== null ? error as { code?: unknown; message?: unknown } : null;
        const message = failure?.code === "failed" && typeof failure.message === "string" ? failure.message : undefined;
        setFormatted({ key, result: stale ? "stale" : "failed", problem: shared ? pickerText.sharedKey(shared) : message });
        if (stale) {
          stalePreview.current = retryKey;
          refreshSnapshot();
        }
      } });
    return () => { live = false; };
  }, [previewKey]);
  return { formatted, chosenKey, chosenRevision, keyFormat, shown, previewProblem, displayedOutput };
}
