import type { Copied, Density, LibraryStatus, UpdateProgress } from "../../api";
import { Progress } from "../../ui/Progress";
import { Button } from "../../ui/Button";
import { densityNames, operationFailures, pickerText, updateText } from "../../messages";

/** An action whose failure the picker reports in its notices. */
export type Operation = keyof typeof operationFailures;

/** Settings problems, failed actions, search and copy errors, and what was
 * copied. Library failures belong to the footer's controls. */
export function Notices({ library, failures, error, citationProblem, copyError, copied, showCopiedText, onDismissCopied, updateProgress, missing = [], densityAnnouncement }: {
  densityAnnouncement?: Density;
  updateProgress?: UpdateProgress | null;
  library: LibraryStatus | null | undefined;
  failures: Partial<Record<Operation, boolean>>;
  error: string | null;
  citationProblem?: string | null;
  missing?: string[];
  copyError: string | null;
  copied: Copied | null;
  showCopiedText: boolean;
  onDismissCopied: () => void;
}) {
  return <>
    <p className="visually-hidden" role="status" aria-label={pickerText.viewChanges} aria-live="polite" aria-atomic="true">
      {densityAnnouncement === undefined ? "" : pickerText.density(densityNames[densityAnnouncement])}
    </p>
    <div className="notices">
      {updateProgress && (updateProgress.phase === "failed" ? <p role="alert">{updateText.failed}</p> :
        <Progress label={updateText[updateProgress.phase]} value={updateProgress.phase === "downloading" && updateProgress.total
          ? Math.min(100, 100 * updateProgress.received / updateProgress.total) : undefined} />)}
      {library?.settings_notice && <p role="alert">{library.settings_notice}</p>}
      {(Object.keys(failures) as Operation[]).filter((operation) => failures[operation] && !libraryOperations.has(operation)).map((operation) => (
        <p role="alert" key={operation}>{operationFailures[operation]}</p>
      ))}
      {error && <p role="alert">{error}</p>}
      {citationProblem && <p role="alert">{citationProblem}</p>}
      {missing.length > 0 && <p role="status">{pickerText.missingCitations(missing)}</p>}
      {copyError && <p role="alert">{copyError}</p>}
      {copied && (
        <div role="status" className="copy-notice">
          <p>
            {pickerText.copied}{showCopiedText && <> <code>{copied.text}</code></>}
          </p>
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
