import { Icon } from "../../icons";
import type { Copied, Density, LibraryStatus, UpdateProgress } from "../../api";
import { Progress } from "../../ui/Progress";
import { Button } from "../../ui/Button";
import { densityNames, operationFailures, pickerText, updateText } from "../../messages";
import "../../ui/CopyReceipt.css";
import "./Notices.css";

/** An action whose failure the picker reports in its notices. */
export type Operation = keyof typeof operationFailures;

/** Settings problems, failed actions, search and copy errors, and what was
 * copied. Library failures belong to the footer's controls. */
export function Notices({ library, failures, error, citationProblem, copyError, copied, confirmation, onDismissCopied, updateProgress, missing = [], densityAnnouncement, feedback }: {
  feedback?: string | null;
  densityAnnouncement?: Density;
  updateProgress?: UpdateProgress | null;
  library: LibraryStatus | null | undefined;
  failures: Partial<Record<Operation, boolean>>;
  error: string | null;
  citationProblem?: string | null;
  missing?: string[];
  copyError: string | null;
  copied: Copied | null;
  confirmation?: Copied | null;
  onDismissCopied: () => void;
}) {
  return <>
    <p className="visually-hidden" role="status" aria-label={pickerText.viewChanges} aria-live="polite" aria-atomic="true">
      {densityAnnouncement === undefined ? "" : pickerText.density(densityNames[densityAnnouncement])}
    </p>
    <div className="notices">
      {updateProgress && (updateProgress.phase === "failed" ? <p role="alert">{updateText.failed}</p> :
        <Progress title={updateText.title} label={updateText[updateProgress.phase]} detail={updateSize(updateProgress)}
          value={updateProgress.phase === "downloading" && updateProgress.total
            ? Math.min(100, 100 * updateProgress.received / updateProgress.total) : undefined} />)}
      {library?.settings_notice && <p role="alert">{library.settings_notice}</p>}
      {(Object.keys(failures) as Operation[]).filter((operation) => failures[operation] && !libraryOperations.has(operation)).map((operation) => (
        <p role="alert" key={operation}>{operationFailures[operation]}</p>
      ))}
      {error && <p role="alert">{error}</p>}
      {citationProblem && <p role="alert" className="citation-problem" tabIndex={0}>{citationProblem}</p>}
      {missing.length > 0 && <p role="status">{pickerText.missingCitations(missing)}</p>}
      {copyError && <p role="alert">{copyError}</p>}
      {feedback && <p className="copy-notice" role="status">{feedback}</p>}
      {copied && (confirmation || copied.warnings.length > 0) && (
        <div role={confirmation ? "status" : "group"} className="copy-notice">
          {confirmation && !feedback && <p className="copy-receipt" data-outcome="copied">
            <Icon name="check" className="copy-receipt-mark" />
            <strong>{pickerText.copied}</strong><code dir="auto" title={confirmation.text}>
              <span className="copy-receipt-excerpt">{confirmation.summary?.text ?? confirmation.text}</span>
              {Boolean(confirmation.summary?.more) && <span className="copy-receipt-more">+{confirmation.summary!.more} more</span>}
            </code>
          </p>}
          {copied.warnings.length > 0 && (
            <>
              <Button className="dismiss" aria-label={pickerText.dismissWarnings} onPress={onDismissCopied}>
                {pickerText.dismiss}
              </Button>
              <ul aria-label={pickerText.copyWarnings}>
                {copied.warnings.map((w, i) => (
                  <li key={i}>{w}</li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
    </div>
  </>;
}

/** Library failures stay beside the footer's library controls. */
export const libraryOperations = new Set<Operation>(["status", "libraryEvents", "choose", "reload", "zotero", "zoteroGroups"]);

/** "3.1 of 7.4 MB" while a download of known size runs; nothing otherwise. */
function updateSize({ phase, received, total }: UpdateProgress): string | undefined {
  if (phase !== "downloading" || !total) return undefined;
  const mb = (bytes: number) => (bytes / 1e6).toFixed(1);
  return `${mb(Math.min(received, total))} of ${mb(total)} MB`;
}
