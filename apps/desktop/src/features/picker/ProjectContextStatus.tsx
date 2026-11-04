import type { WritingProjectView } from "../../api";
import { projectCheckText, writingProjectText as text } from "../../messages";
import { Button } from "../../ui/Button";
import { listedOutput, outputName } from "../../format/outputs";
import "./ProjectContextStatus.css";

/** The task's established choices stay visible while newer saved choices wait.
 * Native code decides whether there is an update or retained task settings. */
export function WritingProjectTaskStatus({ project, onApply, disabled = false }: {
  project: WritingProjectView | null; onApply?: () => void; disabled?: boolean;
}) {
  if (!project || !project.pending && !project.retained && !project.saved_problem) return null;
  const choice = listedOutput(project.output);
  const using = text.using(choice ? outputName(choice) : text.unsupported, project.bibliography);
  return <div className="notices project-context-status" role="status">
    <p className="project-context-message" dir="auto">{project.saved_problem ?? (project.retained ? text.retained(project.name) : text.changedElsewhere(project.name))} {project.pending?.bibliography_changed && `${text.bibliographyChanged} `}{using}</p>
    {project.pending && !project.retained && onApply && <Button label={text.apply} isDisabled={disabled} onPress={onApply} />}
  </div>;
}

/** A paused key check has a deliberate one-off action; a known conflict never
 * offers that action. Recovery and insertion remain separately owned by Ibis. */
export function ProjectCheckStatus({ check, message, checkedAtMs, onRetry, onLocate, onUnverified, paste = false, disabled = false }: {
  check: "paused" | "conflict" | "retained" | null; message: string | null; checkedAtMs?: number | null;
  onRetry?: () => void; onLocate?: () => void; onUnverified?: () => void;
  paste?: boolean; disabled?: boolean;
}) {
  if (!check || !message) return null;
  const date = checkedAtMs === null || checkedAtMs === undefined ? null : new Date(checkedAtMs);
  const checkedAt = date && Number.isFinite(date.getTime()) ? new Intl.DateTimeFormat(undefined, { dateStyle: "short", timeStyle: "short" }).format(date) : null;
  return <div className="notices project-context-status" role={check === "conflict" ? "alert" : "status"}>
    <p className="project-context-message">{message}{check === "retained" && ` ${checkedAt ? projectCheckText.lastChecked(checkedAt) : projectCheckText.unknownCheckTime}`}</p>
    {onRetry && <Button label={projectCheckText.retry} isDisabled={disabled} onPress={onRetry} />}
    {onLocate && <Button label={projectCheckText.locate} isDisabled={disabled} onPress={onLocate} />}
    {check === "paused" && onUnverified && <Button label={paste ? projectCheckText.pasteUnverified : projectCheckText.copyUnverified}
      isDisabled={disabled} onPress={onUnverified} />}
  </div>;
}
