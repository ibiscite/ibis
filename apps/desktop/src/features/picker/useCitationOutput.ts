// What Enter would paste for the chosen references: the formatted or key preview,
// tagged with the choice, style and library it was made for.
import { useEffect, useState } from "react";
import * as nativeApi from "../../api";
import type { CopyFormat, LibraryStatus } from "../../api";
import { formatCitation, type FormattedCitation } from "../../format/formatted";
import { parse } from "../../format/markup";
import type { StyleId } from "../../format/styles";
import { pickerText } from "../../messages";
import { choiceKey, styleOf, type Choice, type CitationReference } from "./choice";

export function useCitationOutput({ chosen, library, libraryVersion, format, style, showPastePreview, cslItems, citationText, previewFileCitation, refreshSnapshot }: {
  /** The selection, or the highlighted result when nothing is selected. */
  chosen: CitationReference[];
  library: LibraryStatus | null | undefined;
  libraryVersion: number;
  format: Choice;
  /** The citation style, when one is chosen and formatting is available. */
  style: StyleId | null;
  showPastePreview: boolean;
  cslItems?: typeof nativeApi.cslItems;
  citationText?: typeof nativeApi.citationText;
  previewFileCitation?: typeof nativeApi.previewFileCitation;
  /** Reads the library status again when the preview meets a newer snapshot. */
  refreshSnapshot: () => void;
}) {
  // Tagged with the style, choice and library it was made for, so Enter never
  // copies a preview of an earlier choice.
  const [formatted, setFormatted] = useState<{ key: string; result: (FormattedCitation & { fileTicket?: string; fileHash?: string }) | "failed" | "stale"; problem?: string } | null>(null);
  const chosenKey = choiceKey(chosen);
  const chosenRevision = chosen[0]?.revision;
  const nativeFile = Boolean(library?.file_engine && previewFileCitation);
  const staleSelection = !nativeFile && chosen.length > 0 && (!chosenRevision || chosen.some((record) => record.revision !== chosenRevision) || chosenRevision !== library?.revision);
  // A key syntax is previewed by the backend without touching the clipboard.
  const keyFormat = styleOf(format) === null && (citationText || nativeFile) ? (format as CopyFormat) : null;
  // A hidden key preview is never fetched: key copying asks the backend itself.
  const keyPreview = showPastePreview ? keyFormat : null;
  const previewKey = `${style ?? (nativeFile ? format : keyPreview)}\u0002${libraryVersion}\u0002${chosenKey}`;
  // What Enter would copy now; null while it is being formatted.
  const shown = staleSelection ? "stale" : formatted?.key === previewKey ? formatted.result : null;
  // Why the preview failed, when the user can fix it.
  const previewProblem = shown === "failed" ? formatted?.problem : undefined;
  // Visibility changes presentation only. Key copying also survives a failed
  // preview, unless the failure is one copying would meet too.
  const displayedOutput = showPastePreview && chosen.length > 0 && (style || (keyFormat && (shown !== "failed" || previewProblem))) ? shown : undefined;
  // Format the preview again whenever the choice or the style changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: previewKey covers the choice, its locators, revision and the style or key format, so it is the only trigger.
  useEffect(() => {
    if (chosen.length === 0 || staleSelection || !chosenRevision) return;
    let live = true;
    const key = previewKey;
    const replaced = new AbortController();
    const ids = chosen.map((r) => r.id);
    const items = chosen.map((r) => ({ id: r.id, locator: r.locator ?? null }));
    let made: Promise<FormattedCitation & { fileTicket?: string; fileHash?: string }>;
    if (nativeFile && previewFileCitation) made = previewFileCitation(chosen.map((r) => ({ id: r.id, cite: r.cite ?? "", locator: r.locator ?? null })), keyFormat ?? "pandoc", style, chosenRevision).then((result) => ({ markup: result.html ? parse(result.html) : [result.text], text: result.text, html: result.html ?? "", warnings: result.warnings, fileTicket: result.ticket, fileHash: result.content_hash }));
    else
    if (style && cslItems) made = cslItems(ids as nativeApi.RecordId[], chosenRevision).then((csl) => formatCitation(csl, style, { signal: replaced.signal, cites: chosen.map((r) => r.locator ? { locator: r.locator.value, label: r.locator.label } : {}) }));
    else if (keyPreview && citationText) made = citationText(items as nativeApi.CitationSelection[], keyPreview, chosenRevision).then(({ text, warnings }) => ({ markup: [], text, html: "", warnings }));
    else return;
    made
      .then((result) => { if (live) setFormatted({ key, result }); })
      .catch((error) => { if (live) {
        const stale = nativeApi.isSnapshotChanged(error);
        const shared = nativeApi.sharedCitationKey(error);
        setFormatted({ key, result: stale ? "stale" : "failed", problem: shared ? pickerText.sharedKey(shared) : undefined });
        if (stale) refreshSnapshot();
      } });
    return () => { live = false; replaced.abort(); };
  }, [previewKey]);
  return { formatted, chosenKey, chosenRevision, keyFormat, shown, previewProblem, displayedOutput };
}
