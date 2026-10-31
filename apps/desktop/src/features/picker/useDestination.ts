// One-time output → remembered app choice → app suggestion → global default.
import { useCallback, useRef, useState } from "react";
import type * as nativeApi from "../../api";
import type { AppOutput, AppPaste, CopyFormat, PickerDestination } from "../../api";
import { choiceName, styleOf, type Choice } from "./choice";
type CopyToken = { opening: number; choice: number; delivery: number };

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
  const [picked, setPicked] = useState(false);
  const reads = useRef(0);
  const openings = useRef(0);
  // Settings refreshes can supersede the return's read within this opening.
  const preserveOpeningOutput = useRef(false);
  const capturedId = useRef<string | null | undefined>(undefined);
  const choices = useRef(0);
  const delivery = useRef(0);
  const lastPolicy = useRef<string | undefined>(undefined);
  const preparedFormat = useRef<Choice>(fallback);
  const saves = useRef(Promise.resolve());
  const read = useCallback((reset = true, preserveOutput = false) => {
    if (!pickerDestination) return;
    const request = ++reads.current;
    if (reset) {
      openings.current += 1;
      lastPolicy.current = undefined;
      preserveOpeningOutput.current = preserveOutput;
      setOnce(preserveOutput ? preparedFormat.current : null);
      setOnceOnly(preserveOutput);
      setPicked(false);
    }
    setReady(false);
    // A reopening reads after outstanding saves, so it sees the durable choice.
    void saves.current.then(pickerDestination).then((next) => {
      if (request !== reads.current) return;
      if (!reset && !preserveOpeningOutput.current && capturedId.current !== undefined && capturedId.current !== next.app_id) {
        openings.current += 1;
        setOnce(null);
        setOnceOnly(false);
        setPicked(false);
      }
      capturedId.current = next.app_id;
      const policy = JSON.stringify([next.app_id, next.app_identity, next.paste, next.can_paste, next.unreadable]);
      if (!reset && lastPolicy.current !== undefined && lastPolicy.current !== policy) delivery.current += 1;
      lastPolicy.current = policy;
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
  preparedFormat.current = format;
  const save = (action: () => Promise<unknown>, apply: () => void, clearOnce = true) => {
    const opening = openings.current;
    const choice = choices.current;
    setSaving(true);
    const pending = saves.current.then(action).then(() => {
      if (opening !== openings.current) return;
      apply();
      if (clearOnce && choice === choices.current) { preserveOpeningOutput.current = false; setOnce(null); }
      report("appChoice", false);
    }).catch(() => { if (opening === openings.current) report("appChoice"); });
    saves.current = pending;
    void pending.finally(() => { if (saves.current === pending) setSaving(false); });
  };
  const remember = (choice: Choice | null) => {
    const id = destination?.app_id;
    if (!rememberAppOutput || !id || !ready) return;
    const output = choice ? outputOf(choice) : null;
    save(() => rememberAppOutput(id, output, null, destination?.app_identity ?? null), () => {
      setDestination((old) => old && { ...old, remembered: output });
      if (choice === null && destination?.paste || destination?.unreadable) read(false);
    });
  };
  const choose = (choice: Choice) => {
    choices.current += 1;
    setPicked(true);
    setOnce(choice);
    if (ready && app && !onceOnly) remember(choice);
    else if (ready && !app && !onceOnly && (!pickerDestination || destination)) { chooseDefault(choice); setOnce(null); }
  };
  const choosePaste = (paste: AppPaste) => {
    const id = destination?.app_id;
    if (!rememberAppOutput || !id || !ready || saving) return;
    delivery.current += 1;
    lastPolicy.current = undefined;
    // Persisting delivery does not resume the Enter that opened the chooser.
    // Read the native policy back before showing an intended paste in the footer.
    const output = onceOnly ? destination?.remembered ?? null : outputOf(format);
    save(() => rememberAppOutput(id, output, paste, destination?.app_identity ?? null), () => read(false), false);
  };
  const copyToken = (): CopyToken => ({ opening: openings.current, choice: choices.current, delivery: delivery.current });
  const isCurrentCopy = (token: CopyToken) => token.opening === openings.current && token.choice === choices.current && token.delivery === delivery.current;
  const consumeOnce = (token: CopyToken) => {
    if (!isCurrentCopy(token)) return null;
    if (once !== null) setPicked(false);
    preserveOpeningOutput.current = false;
    setOnce(null); setOnceOnly(false);
    return base.current;
  };
  const restoreAutomatic = () => {
    choices.current += 1;
    if (destination?.paste || destination?.unreadable) delivery.current += 1;
    lastPolicy.current = undefined;
    setPicked(false);
    preserveOpeningOutput.current = false;
    setOnceOnly(false);
    if (remembered || destination?.paste) remember(null);
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
    delivery.current += 1;
    lastPolicy.current = undefined;
    setPicked(false);
    save(forgetAppOutputs, () => {
      setDestination((old) => old && { ...old, remembered: null });
      setOnceOnly(false);
      if (destination?.paste || destination?.unreadable) read(false);
    });
  };
  const scope = !picked ? null : once !== null || onceOnly ? `${choiceName(format)} for this citation`
    : remembered ? `${choiceName(format)} remembered for this ${destination?.output_question ?? "app"}`
      : !app ? `${choiceName(format)} used as global default` : null;
  const askPaste = Boolean(rememberAppOutput && destination?.app_id && destination.paste_question);
  const pasteChoices = Boolean(rememberAppOutput && destination?.can_choose_paste);
  const shouldAsk = askPaste || Boolean(rememberAppOutput && destination?.app_id && destination.output_question && !remembered && once === null);
  return { app: rememberAppOutput ? app : null, format, ready, saving, scope, shouldAsk, remembered: remembered !== null, read, choose,
    onceOnly, toggleOnce: () => { choices.current += 1; setOnceOnly((old) => !old); }, copyToken, isCurrentCopy, consumeOnce, restoreAutomatic, useDefault,
    askPaste, pasteChoices, choosePaste, quitAfterSaving, forgetAll: forgetAppOutputs ? forgetAll : null };
}
