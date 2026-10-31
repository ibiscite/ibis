import type { Copied, Density, LibraryStatus, LoginItem, UpdateProgress } from "../../api";
import { Progress } from "../../ui/Progress";
import { Button } from "../../ui/Button";
import { hasRetainedFreshnessProblem } from "../../LibraryStatusLine";
import { densityNames, libraryMessages, loginItemText, operationFailures, pickerText, updateText } from "../../messages";
import { currentPlatform } from "../../platform";
import "../../ui/CopyReceipt.css";
import "./Notices.css";

/** An action whose failure the picker reports in its notices. */
export type Operation = keyof typeof operationFailures;

/** Problems and confirmations above the results: library and settings
 * problems, failed actions, search and copy errors, and what was copied. */
export function Notices({ library, failures, startAtLogin, error, citationProblem, copyError, copied, confirmation, onDismissCopied, updateProgress, missing = [], densityAnnouncement }: {
  densityAnnouncement?: Density;
  updateProgress?: UpdateProgress | null;
  library: LibraryStatus | null | undefined;
  failures: Partial<Record<Operation, boolean>>;
  startAtLogin: LoginItem | null;
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
        <Progress label={updateText[updateProgress.phase]} value={updateProgress.phase === "downloading" && updateProgress.total
          ? Math.min(100, 100 * updateProgress.received / updateProgress.total) : undefined} />)}
      {library?.settings_notice && <p role="alert">{library.settings_notice}</p>}
      {/* A library that could not be opened at all gets the recovery screen instead. */}
      {library?.error && !(library.state === "error" && !library.has_snapshot) && !hasRetainedFreshnessProblem(library) && <p role="alert">{libraryMessages.openError(library.name, library.error, library.zotero)}</p>}
      {(Object.keys(failures) as Operation[]).filter((operation) => failures[operation]).map((operation) => (
        <p role="alert" key={operation}>{operationFailures[operation]}</p>
      ))}
      {startAtLogin?.problem && startAtLogin.problem !== "unsupported" && (
        <p role="alert">{loginItemText.problem(startAtLogin.problem, currentPlatform(), startAtLogin.name)}</p>
      )}
      {error && <p role="alert">{error}</p>}
      {citationProblem && <p role="alert" className="citation-problem" tabIndex={0}>{citationProblem}</p>}
      {missing.length > 0 && <p role="status">{pickerText.missingCitations(missing)}</p>}
      {copyError && <p role="alert">{copyError}</p>}
      {copied && (confirmation || copied.warnings.length > 0) && (
        <div role={confirmation ? "status" : "group"} className="copy-notice">
          {confirmation && <p className="copy-receipt" data-outcome="copied">
            <svg className="copy-receipt-mark" aria-hidden="true" viewBox="0 0 20 20"><path d="m5 10.5 3.2 3.2L15 6.5" /></svg>
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
