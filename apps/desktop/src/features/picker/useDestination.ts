// One-time output → remembered app choice → app suggestion → global default.
import { useCallback, useRef, useState } from "react";
import type * as nativeApi from "../../api";
import type { AppOutput, CopyFormat, PickerDestination } from "../../api";
import { styleOf, type Choice } from "./choice";
type CopyToken = { opening: number; choice: number };

import type { Operation } from "./Notices";

type DestinationApi = Partial<Pick<typeof nativeApi, "pickerDestination" | "rememberAppOutput" | "forgetAppOutputs">>;
export function choiceOf(output: AppOutput | null): Choice | null {
  return output ? output.key ?? `style:${output.style}` : null;
}
export function outputOf(choice: Choice): AppOutput {
  const style = styleOf(choice);
  return style ? { style } : { key: choice as CopyFormat };
}

export function useDestination({ pickerDestination, rememberAppOutput, forgetAppOutputs }: DestinationApi, { fallback, fallbackReady = true, chooseDefault, report }: {
  fallback: Choice;
  fallbackReady?: boolean;
  chooseDefault: (choice: Choice) => void;
  report: (operation: Operation, failed?: boolean) => void;
}) {
  const [destination, setDestination] = useState<PickerDestination | null>(null);
  const [once, setOnce] = useState<Choice | null>(null);
  const [onceOnly, setOnceOnly] = useState(false);
  const [ready, setReady] = useState(!pickerDestination);
  const [saving, setSaving] = useState(false);
  const reads = useRef(0);
  const openings = useRef(0);
  const capturedId = useRef<string | null | undefined>(undefined);
  const choices = useRef(0);
  const saves = useRef(Promise.resolve());
  const read = useCallback((reset = true) => {
    if (!pickerDestination) return;
    const request = ++reads.current;
    if (reset) { openings.current += 1; setOnce(null); setOnceOnly(false); }
    setReady(false);
    // A reopening reads after outstanding saves, so it sees the durable choice.
    void saves.current.then(pickerDestination).then((next) => {
      if (request !== reads.current) return;
      if (!reset && capturedId.current !== undefined && capturedId.current !== next.app_id) {
        openings.current += 1;
        setOnce(null);
        setOnceOnly(false);
      }
      capturedId.current = next.app_id;
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
  const appOutput = destination?.automatic;
  const automatic = appOutput && "style" in appOutput && fallbackReady
    ? `style:${styleOf(fallback) ?? "apa"}` as Choice : choiceOf(appOutput ?? null);
  const baseFormat = remembered ?? automatic ?? fallback;
  const base = useRef(baseFormat);
  base.current = baseFormat;
  const format = once ?? baseFormat;
  const save = (action: () => Promise<unknown>, apply: () => void) => {
    const opening = openings.current;
    const choice = choices.current;
    setSaving(true);
    const pending = saves.current.then(action).then(() => {
      if (opening !== openings.current) return;
      apply();
      if (choice === choices.current) setOnce(null);
      report("appChoice", false);
      report("appChoices", false);
    }).catch(() => { if (opening === openings.current) report("appChoice"); });
    saves.current = pending;
    void pending.finally(() => { if (saves.current === pending) setSaving(false); });
  };
  const remember = (choice: Choice | null) => {
    const id = destination?.app_id;
    if (!rememberAppOutput || !id || !ready) return;
    const output = choice ? outputOf(choice) : null;
    save(() => rememberAppOutput(id, output), () => setDestination((old) => old && { ...old, remembered: output }));
  };
  const choose = (choice: Choice) => {
    choices.current += 1;
    setOnce(choice);
    if (ready && app && !onceOnly) remember(choice);
    else if (ready && !app && !onceOnly && (!pickerDestination || destination)) { chooseDefault(choice); setOnce(null); }
  };
  const copyToken = (): CopyToken => ({ opening: openings.current, choice: choices.current });
  const isCurrentCopy = (token: CopyToken) => token.opening === openings.current && token.choice === choices.current;
  const consumeOnce = (token: CopyToken) => {
    if (!isCurrentCopy(token)) return null;
    setOnce(null); setOnceOnly(false);
    return base.current;
  };
  const restoreAutomatic = () => {
    choices.current += 1;
    setOnceOnly(false);
    if (remembered) remember(null);
    else setOnce(null);
  };
  const useDefault = () => { chooseDefault(format); };
  const quitAfterSaving = async (quit: () => Promise<void>) => {
    let timer: number | undefined;
    await Promise.race([saves.current, new Promise<void>((done) => { timer = window.setTimeout(done, 2_000); })]);
    window.clearTimeout(timer);
    await quit();
  };
  const forgetAll = () => {
    if (!forgetAppOutputs) return;
    choices.current += 1;
    save(forgetAppOutputs, () => { setDestination((old) => old && { ...old, remembered: null }); setOnceOnly(false); });
  };
  return { app: rememberAppOutput ? app : null, format, ready, saving, remembered: remembered !== null, read, choose,
    onceOnly, toggleOnce: () => { choices.current += 1; setOnceOnly((old) => !old); }, copyToken, isCurrentCopy, consumeOnce, restoreAutomatic, useDefault,
    quitAfterSaving, forgetAll: forgetAppOutputs ? forgetAll : null };
}
