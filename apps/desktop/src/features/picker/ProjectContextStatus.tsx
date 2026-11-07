import { say } from "../../i18n";
import { Button } from "../../ui/Button";
import "./ProjectContextStatus.css";

/** The project bibliography beside a key citation. A key known to name a
 * different work is refused; a file that can't be read is only advice, and
 * the citation stays available. Either way the person can retry or find it. */
export function ProjectCheckNotice({ refusal, problem, onRetry, onLocate, disabled = false }: {
  refusal: string | null; problem: string | null; onRetry?: () => void; onLocate?: () => void; disabled?: boolean;
}) {
  const message = refusal ?? problem;
  if (!message) return null;
  return <div className="notices project-context-status" role={refusal ? "alert" : "status"}>
    <p className="project-context-message" dir="auto">{message}</p>
    {onRetry && <Button label={say("project-check-retry")} isDisabled={disabled} onPress={onRetry} />}
    {onLocate && <Button label={say("project-check-locate")} isDisabled={disabled} onPress={onLocate} />}
  </div>;
}
