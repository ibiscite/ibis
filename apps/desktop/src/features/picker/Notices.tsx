import type { Copied, LibraryStatus, LoginItem } from "../../api";
import { Button } from "../../ui/Button";
import { hasRetainedFreshnessProblem } from "../../LibraryStatusLine";
import { libraryMessages, loginItemText, operationFailures, pickerText } from "../../messages";
import { currentPlatform } from "../../platform";

/** An action whose failure the picker reports in its notices. */
export type Operation = keyof typeof operationFailures;

/** Problems and confirmations above the results: library and settings
 * problems, failed actions, search and copy errors, and what was copied. */
export function Notices({ library, failures, startAtLogin, error, copyError, copied, showCopiedText, onDismissCopied }: {
  library: LibraryStatus | null | undefined;
  failures: Partial<Record<Operation, boolean>>;
  startAtLogin: LoginItem | null;
  error: string | null;
  copyError: string | null;
  copied: Copied | null;
  showCopiedText: boolean;
  onDismissCopied: () => void;
}) {
  return (
    <div className="notices">
      {library?.settings_notice && <p role="alert">{library.settings_notice}</p>}
      {library?.error && !hasRetainedFreshnessProblem(library) && <p role="alert">{libraryMessages.openError(library.name, library.error, library.zotero)}</p>}
      {(Object.keys(failures) as Operation[]).filter((operation) => failures[operation]).map((operation) => (
        <p role="alert" key={operation}>{operationFailures[operation]}</p>
      ))}
      {startAtLogin?.problem && startAtLogin.problem !== "unsupported" && (
        <p role="alert">{loginItemText.problem(startAtLogin.problem, currentPlatform(), startAtLogin.name)}</p>
      )}
      {error && <p role="alert">{error}</p>}
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
  );
}
