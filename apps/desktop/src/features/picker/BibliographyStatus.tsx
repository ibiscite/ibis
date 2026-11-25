import { say } from "../../i18n";
import { Button } from "../../ui/Button";
import { LineNotice } from "../../ui/Frame";

type CheckFixes = { onRetry?: () => void; onLocate?: () => void; disabled?: boolean };

/** Retry the check, or find the bibliography: the fixes for either kind of answer. */
export function BibliographyCheckFixes({ onRetry, onLocate, disabled = false }: CheckFixes) {
  return <>
    {onRetry && <Button size="small" label={say("project-check-retry")} isDisabled={disabled} onPress={onRetry} />}
    {onLocate && <Button size="small" label={say("project-check-locate")} isDisabled={disabled} onPress={onLocate} />}
  </>;
}

/** The bibliography beside a key citation. A key known to name a different
 * work is refused, and the delivery bar says so with these fixes; a file that
 * can't be read is only advice, in the notice line, and the citation stays
 * available. The fixes show once, beside the refusal when there is one. */
export function bibliographyCheckNotice({ refused, problem, ...fixes }: CheckFixes & { refused: boolean; problem: string | null }) {
  if (!problem) return null;
  return <LineNotice key="bibliography-check" actions={refused ? undefined : <BibliographyCheckFixes {...fixes} />}>{problem}</LineNotice>;
}
