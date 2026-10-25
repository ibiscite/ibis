import { useEffect, useState } from "react";
import { keysFor } from "../../keys";
import { startText } from "../../messages";
import { Button } from "../../ui/Button";
import { KeyHint } from "../../ui/KeyCaps";
import "./CompactStart.css";

const { tips } = startText;
// Each start shows the tip after the last one shown.
let nextTip = 0;

/** The empty query's one hint line (docs/design.md, C11): a tip, label then
 * keys, and its place in the round, which shows the next one. */
export function CompactStart() {
  const [tip, setTip] = useState(() => nextTip % tips.length);
  useEffect(() => { nextTip = tip + 1; }, [tip]);
  const { label, action } = tips[tip]!;
  const n = tip + 1;
  return <div className="compact-start">
    <KeyHint label={label} keys={keysFor(action)} />
    <Button variant="quiet" size="small" className="compact-next" aria-label={startText.nextTip(n, tips.length)}
      onPress={() => setTip((tip + 1) % tips.length)}>{startText.tipCount(n, tips.length)}</Button>
  </div>;
}
