// The output follows the app the picker was opened from (#1202): its automatic
// output, or the one the person chose to always use there. A pick applies just
// this once, until the picker is shown again; "Always use for …" keeps it.
import { useCallback, useRef, useState } from "react";
import type * as nativeApi from "../../api";
import type { AppOutput, CopyFormat, PickerDestination } from "../../api";
import { styleOf, type Choice } from "./choice";
import type { Operation } from "./Notices";

type DestinationApi = Partial<Pick<typeof nativeApi, "pickerDestination" | "rememberAppOutput" | "forgetAppOutputs">>;

export function choiceOf(output: AppOutput | null): Choice | null {
  if (!output) return null;
  return output.key ?? `style:${output.style}`;
}

export function outputOf(choice: Choice): AppOutput {
  const style = styleOf(choice);
  return style ? { style } : { key: choice as CopyFormat };
}

export function useDestination({ pickerDestination, rememberAppOutput, forgetAppOutputs }: DestinationApi, { fallback, chooseDefault, report, perform }: {
  /** The person's default format, for apps Ibis does not know. */
  fallback: Choice;
  /** Saves a new default format, as picking one did before apps were known. */
  chooseDefault: (choice: Choice) => void;
  report: (operation: Operation, failed?: boolean) => void;
  perform: (operation: Operation, action: () => Promise<unknown>) => Promise<void>;
}) {
  const [destination, setDestination] = useState<PickerDestination | null>(null);
  const [once, setOnce] = useState<Choice | null>(null);
  // A copy waits while the app's output is read, so it never uses the last app's.
  const [ready, setReady] = useState(true);
  const reads = useRef(0);
  const read = useCallback(() => {
    if (!pickerDestination) return;
    const request = ++reads.current;
    setOnce(null);
    setReady(false);
    pickerDestination().then((next) => {
      if (request !== reads.current) return;
      setDestination(next);
      report("appChoices", next.unreadable);
    }).catch(() => {
      if (request !== reads.current) return;
      setDestination(null);
      report("appChoices");
    }).finally(() => { if (request === reads.current) setReady(true); });
  }, [pickerDestination, report]);
  const app = destination?.app ?? null;
  const remembered = choiceOf(destination?.remembered ?? null);
  const automatic = choiceOf(destination?.automatic ?? null);
  const format = once ?? remembered ?? automatic ?? fallback;
  const remember = (choice: Choice | null) => {
    if (!rememberAppOutput) return;
    reads.current += 1;
    setReady(true);
    setOnce(null);
    setDestination((old) => old && { ...old, remembered: choice ? outputOf(choice) : null });
    void perform("appChoice", () => rememberAppOutput(choice ? outputOf(choice) : null));
  };
  const choose = (choice: Choice) => {
    if (app && remembered) remember(choice);
    else if (app && automatic) setOnce(choice);
    else {
      setOnce(null);
      chooseDefault(choice);
    }
  };
  /** Checks or unchecks "Always use for …"; unchecking goes back to Automatic. */
  const toggleAlways = () => remember(remembered ? null : format);
  const forgetAll = () => {
    if (!forgetAppOutputs) return;
    setOnce(null);
    setDestination((old) => old && { ...old, remembered: null });
    void perform("appChoice", forgetAppOutputs);
  };
  return { app: rememberAppOutput ? app : null, format, ready, remembered: remembered !== null, read, choose, toggleAlways,
    forgetAll: forgetAppOutputs ? forgetAll : null };
}
