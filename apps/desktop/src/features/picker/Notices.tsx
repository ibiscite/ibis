import type { Copied, Density, LibraryStatus, LoginItem, UpdateProgress } from "../../api";
import { Progress } from "../../ui/Progress";
import { Button } from "../../ui/Button";
import { hasRetainedFreshnessProblem } from "../../LibraryStatusLine";
import { densityNames, libraryMessages, loginItemText, operationFailures, pickerText, updateText } from "../../messages";
import { currentPlatform } from "../../platform";
import { noticeTitle } from "../../copyFeedback";

/** An action whose failure the picker reports in its notices. */
export type Operation = keyof typeof operationFailures;

/** Problems and confirmations above the results: library and settings
 * problems, failed actions, search and copy errors, and what was copied. */
export function Notices({ library, failures, startAtLogin, error, copyError, copied, showCopiedText, onDismissCopied, updateProgress, missing = [], densityAnnouncement }: {
  densityAnnouncement?: Density;
  updateProgress?: UpdateProgress | null;
  library: LibraryStatus | null | undefined;
  failures: Partial<Record<Operation, boolean>>;
  startAtLogin: LoginItem | null;
  error: string | null;
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
      {/* A library that could not be opened at all gets the recovery screen instead. */}
      {library?.error && !(library.state === "error" && !library.has_snapshot) && !hasRetainedFreshnessProblem(library) && <p role="alert">{libraryMessages.openError(library.name, library.error, library.zotero)}</p>}
      {(Object.keys(failures) as Operation[]).filter((operation) => failures[operation]).map((operation) => (
        <p role="alert" key={operation}>{operationFailures[operation]}</p>
      ))}
      {startAtLogin?.problem && startAtLogin.problem !== "unsupported" && (
        <p role="alert">{loginItemText.problem(startAtLogin.problem, currentPlatform(), startAtLogin.name)}</p>
      )}
      {error && <p role="alert">{error}</p>}
      {missing.length > 0 && <p role="status">{pickerText.missingCitations(missing)}</p>}
      {copyError && <p role="alert">{copyError}</p>}
      {copied && (
        <div role="status" className="copy-notice">
          <p>
            {noticeTitle({ delivered: { outcome: "copied", reason: null }, text: "", paste_hint: copied.warnings.length === 0 && copied.paste_hint })}{showCopiedText && <> <code>{copied.text}</code></>}
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
