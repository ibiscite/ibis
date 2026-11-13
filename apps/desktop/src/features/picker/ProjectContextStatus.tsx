import { say } from "../../i18n";
import { Button } from "../../ui/Button";
import { LineNotice } from "../../ui/Frame";

/** The project bibliography beside a key citation. A key known to name a
 * different work is refused; a file that can't be read is only advice, and
 * the citation stays available. Either way the person can retry or find it. */
export function projectCheckNotice({ refusal, problem, onRetry, onLocate, disabled = false }: {
  refusal: string | null; problem: string | null; onRetry?: () => void; onLocate?: () => void; disabled?: boolean;
}) {
  const message = refusal ?? problem;
  if (!message) return null;
  return <LineNotice key="project-check" className="project-context-status" role={refusal ? "alert" : "status"} actions={<>
    {onRetry && <Button size="small" label={say("project-check-retry")} isDisabled={disabled} onPress={onRetry} />}
    {onLocate && <Button size="small" label={say("project-check-locate")} isDisabled={disabled} onPress={onLocate} />}
  </>}>{message}</LineNotice>;
}
