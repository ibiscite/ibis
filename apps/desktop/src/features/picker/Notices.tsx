import type { ReactNode } from "react";
import type { Copied, LibraryStatus, UpdateProgress } from "../../api";
import { FrameNotice, LineNotice } from "../../ui/Frame";
import { QuietNotice } from "../../ui/NoticeSlot";
import { Progress } from "../../ui/Progress";
import { Button } from "../../ui/Button";
import { isolate, type MessageId, say } from "../../i18n";

/** An action whose failure the picker reports in its notices. */
/** What failed, by operation: each says what didn't happen and what to do. */
export const OPERATION_FAILURES = {
  recentHistory: "operation-failures-recent-history",
  project: "operation-failures-project",
  rowContext: "operation-failures-row-context",
  status: "operation-failures-status",
  shortcut: "operation-failures-shortcut",
  choose: "operation-failures-choose",
  dropRefused: "operation-failures-drop-refused",
  zotero: "operation-failures-zotero",
  samples: "operation-failures-samples",
  zoteroGroups: "operation-failures-zotero-groups",
  reload: "operation-failures-reload",
  hide: "operation-failures-hide",
  pin: "operation-failures-pin",
  preferences: "operation-failures-preferences",
  windowSize: "operation-failures-window-size",
  quit: "operation-failures-quit",
  updateEvents: "operation-failures-update-events",
  pickerEvents: "operation-failures-picker-events",
  libraryEvents: "operation-failures-library-events",
  findSources: "operation-failures-find-sources",
  openFound: "operation-failures-open-found",
  settings: "operation-failures-settings",
  appChoices: "operation-failures-app-choices",
  appRead: "operation-failures-app-read",
  appChoice: "operation-failures-app-choice",
  appDelivery: "operation-failures-app-delivery",
} as const satisfies Record<string, MessageId>;
export type Operation = keyof typeof OPERATION_FAILURES;

/** Settings problems, failed actions, search and copy errors, and what was
 * copied, most important first, for the frame's notice line (ui/Frame). Library
 * failures belong to the footer's controls; a refusal to format belongs to the
 * shelf, which says it beside the citation with Details. */
export function notices({ library, failures, error, copyError, copied, confirmation, onDismissCopied, missing = [], projectNotice, editorNotice }: {
  /** What became of the last project entry copy. */
  projectNotice?: string | null;
  /** The format an editor waiting on the endpoint asked for. */
  editorNotice?: string | null;
  library: LibraryStatus | null | undefined;
  failures: Partial<Record<Operation, boolean>>;
  error: string | null;
  missing?: string[];
  copyError: string | null;
  copied: Copied | null;
  confirmation?: Copied | null;
  onDismissCopied: () => void;
}): ReactNode[] {
  // What the person's last action caused comes first, so a standing problem
  // never hides a new one; the standing ones follow.
  return [
    error && <LineNotice key="error" role="alert">{error}</LineNotice>,
    copyError && <LineNotice key="copy-error" role="alert">{copyError}</LineNotice>,
    copied && copied.warnings.length > 0 && <FrameNotice key="warnings" className="copy-notice" role="group">
      <ul className="ui-line-words" tabIndex={0} aria-label={say("picker-copy-warnings")} title={copied.warnings.join("\n")}>
        {copied.warnings.map((w, i) => <li key={i}>{w}</li>)}
      </ul>
      <Button size="small" aria-label={say("picker-dismiss-warnings")} onPress={onDismissCopied}>{say("picker-dismiss")}</Button>
    </FrameNotice>,
    copied && confirmation && <FrameNotice key="copied" className="copy-notice" role="status">
      <p className="ui-line-words" data-outcome="copied"><strong>{say("picker-copied")}</strong> <code dir="auto" title={confirmation.text}>
        {confirmation.summary?.text ?? confirmation.text}
        {Boolean(confirmation.summary?.more) && <span> +{confirmation.summary!.more} more</span>}
      </code></p>
    </FrameNotice>,
    ...(Object.keys(failures) as Operation[]).filter((operation) => failures[operation] && !libraryOperations.has(operation)).map((operation) => (
      <LineNotice role="alert" key={operation}>{say(OPERATION_FAILURES[operation])}</LineNotice>
    )),
    missing.length > 0 && <LineNotice key="missing">{say("picker-missing-citations", { keys: missing.join(", ") })}</LineNotice>,
    editorNotice && <LineNotice key="editor" className="editor-notice">{say("picker-editor-waiting", { format: isolate(editorNotice) })}</LineNotice>,
    projectNotice && <LineNotice key="project">{projectNotice}</LineNotice>,
    library?.settings_notice && <LineNotice key="settings" role="alert">{library.settings_notice}</LineNotice>,
  ].filter(Boolean);
}

/** An update in the notice slot, so no row moves while it runs. */
export function UpdateNotice({ progress, paused, onSeen }: { progress: UpdateProgress; paused?: boolean; onSeen?: () => void }) {
  if (progress.phase === "failed") return <QuietNotice role="alert" paused={paused} onExpire={onSeen}>{say("update-failed")}</QuietNotice>;
  return <Progress title={say("update-title")} label={say(progress.phase === "downloading" ? "update-downloading" : "update-installing")} detail={updateSize(progress)}
    value={progress.phase === "downloading" && progress.total ? Math.min(100, 100 * progress.received / progress.total) : undefined} />;
}

/** Library failures stay beside the footer's library controls. */
export const libraryOperations = new Set<Operation>(["status", "libraryEvents", "choose", "reload", "zotero", "zoteroGroups"]);

/** "3.1 of 7.4 MB" while a download of known size runs; nothing otherwise. */
function updateSize({ phase, received, total }: UpdateProgress): string | undefined {
  if (phase !== "downloading" || !total) return undefined;
  const mb = (bytes: number) => (bytes / 1e6).toFixed(1);
  return `${mb(Math.min(received, total))} of ${mb(total)} MB`;
}
