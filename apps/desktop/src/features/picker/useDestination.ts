// The app's remembered format → the app table's suggestion → the default.
import { useCallback, useRef, useState } from "react";
import type * as nativeApi from "../../api";
import type { AppOutput, AppPaste, CopyFormat, PickerDestination } from "../../api";
import { styleOf, type Choice } from "./choice";
type CopyToken = { opening: number; choice: number; delivery: number };

import type { Operation } from "./Notices";

type DestinationApi = Partial<Pick<typeof nativeApi, "pickerDestination" | "rememberAppOutput" | "editWritingProject">>;
/** Why the next paste uses its format. */
export type FormatSource = "remembered" | "suggested" | "default" | "held";
/** Where a format pick goes: this app, the default, or only the next paste. */
export type FormatPick = "app" | "default" | "paste";
export function choiceOf(output: AppOutput | null): Choice | null {
  return output ? output.key ?? `style:${output.style}` : null;
}
export function outputOf(choice: Choice): AppOutput {
  const style = styleOf(choice);
  return style ? { style } : { key: choice as CopyFormat };
}

export function useDestination({ pickerDestination, rememberAppOutput, editWritingProject }: DestinationApi, { fallback, fallbackReady = true, chooseDefault, report }: {
  fallback: Choice;
  fallbackReady?: boolean;
  chooseDefault: (choice: Choice) => void;
  report: (operation: Operation, failed?: boolean) => void;
}) {
  const [destination, setDestination] = useState<PickerDestination | null>(null);
  // Beats the app's format until saved or copied: a pick being saved, a pick
  // that could not be saved, or the output a permission return prepared.
  const [held, setHeld] = useState<Choice | null>(null);
  const [ready, setReady] = useState(!pickerDestination);
  const [saving, setSaving] = useState(false);
  const [readProblem, setReadProblem] = useState<string | null>(null);
  const reads = useRef(0);
  const openings = useRef(0);
  // Settings refreshes can supersede the return's read within this opening.
  const preserveOpeningOutput = useRef(false);
  const capturedId = useRef<string | null | undefined>(undefined);
  const capturedContext = useRef<string | undefined>(undefined);
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
      setHeld(preserveOutput ? preparedFormat.current : null);
      report("appChoice", false);
    }
    setReady(false);
    // A reopening reads after outstanding saves, so it sees the durable choice.
    void saves.current.then(pickerDestination).then((next) => {
      if (request !== reads.current) return;
      if (!reset && !preserveOpeningOutput.current && capturedId.current !== undefined && capturedId.current !== next.app_id) {
        openings.current += 1;
        setHeld(null);
      }
      capturedId.current = next.app_id;
      const context = JSON.stringify(next.context_version ?? null);
      if (capturedContext.current !== undefined && capturedContext.current !== context) {
        choices.current += 1;
        setHeld(null);
        preserveOpeningOutput.current = false;
      }
      capturedContext.current = context;
      const policy = JSON.stringify([next.app_id, next.app_identity, next.paste, next.can_paste, next.unreadable]);
      if (!reset && lastPolicy.current !== undefined && lastPolicy.current !== policy) delivery.current += 1;
      lastPolicy.current = policy;
      setDestination(next);
      setReadProblem(null);
      report("appRead", false);
      report("appChoices", next.unreadable);
    }).catch((error: unknown) => {
      if (request !== reads.current) return;
      setDestination(null);
      setReadProblem(typeof error === "string" ? error : null);
      report("appChoices", false);
      if (typeof error === "string") report("appRead", false);
      else report("appRead");
    }).finally(() => { if (request === reads.current) setReady(true); });
  }, [pickerDestination, report]);
  const app = rememberAppOutput ? destination?.app ?? null : null;
  const remembered = choiceOf(destination?.remembered ?? null);
  const appOutput = destination?.automatic;
  const suggested = appOutput && "style" in appOutput && fallbackReady
    ? `style:${styleOf(fallback) ?? "apa"}` as Choice : choiceOf(appOutput ?? null);
  const writingProject = destination?.writing_project ?? null;
  const projectFormat = choiceOf(writingProject?.output ?? null);
  const baseFormat = projectFormat ?? remembered ?? suggested ?? fallback;
  const format = held ?? baseFormat;
  preparedFormat.current = format;
  const save = (action: () => Promise<unknown>, apply: () => void, clearHeld = true) => {
    const opening = openings.current;
    const choice = choices.current;
    setSaving(true);
    const pending = saves.current.then(action).then(() => {
      if (opening !== openings.current) return;
      apply();
      if (clearHeld && choice === choices.current) { preserveOpeningOutput.current = false; setHeld(null); }
      report("appChoice", false);
    }).catch(() => { if (opening === openings.current) report("appChoice"); });
    saves.current = pending;
    void pending.finally(() => { if (saves.current === pending) setSaving(false); });
  };
  const remember = (choice: Choice) => {
    const id = destination?.app_id;
    if (!rememberAppOutput || !id) return;
    const output = outputOf(choice);
    save(() => rememberAppOutput(id, output, null, destination?.app_identity ?? null), () => {
      setDestination((old) => old && { ...old, remembered: output });
      if (destination?.unreadable) read(false);
    });
  };
  // An app that could not be read may still be a real one, so a pick cannot change the default.
  const pick: FormatPick = app ? "app" : !pickerDestination || destination ? "default" : "paste";
  const choose = (choice: Choice) => {
    choices.current += 1;
    if (writingProject && destination?.writing_context && editWritingProject) {
      const context = destination.writing_context;
      save(() => editWritingProject(context, { kind: "output", id: writingProject.id, revision: writingProject.revision, output: outputOf(choice) }), () => read(false));
      return;
    }
    setHeld(choice);
    if (!ready) return;
    if (pick === "app") remember(choice);
    else if (pick === "default") { chooseDefault(choice); setHeld(null); }
  };
  /** This citation's format, and only this one: held, never saved, consumed by the copy. */
  const chooseOnce = (choice: Choice) => {
    choices.current += 1;
    setHeld(choice);
  };
  const choosePaste = (paste: AppPaste, asked = false) => {
    const id = destination?.app_id;
    if (!rememberAppOutput || !id || !ready || saving) return;
    delivery.current += 1;
    lastPolicy.current = undefined;
    // Persisting delivery does not resume the Enter that opened the chooser.
    // Read the native policy back before showing an intended paste in the footer.
    // Answering an open first-paste question accepts the format it showed, so it is not asked again.
    const output = asked && !writingProject && held === null && destination?.output_question && !remembered ? outputOf(format) : null;
    save(() => rememberAppOutput(id, output, paste, destination?.app_identity ?? null), () => read(false), false);
  };
  const copyToken = (): CopyToken => ({ opening: openings.current, choice: choices.current, delivery: delivery.current });
  const isCurrentCopy = (token: CopyToken) => token.opening === openings.current && token.choice === choices.current && token.delivery === delivery.current;
  const consumeHeld = (token: CopyToken) => {
    if (!isCurrentCopy(token)) return;
    preserveOpeningOutput.current = false;
    setHeld(null);
  };
  const quitAfterSaving = async (quit: () => Promise<void>) => {
    let timer: number | undefined;
    await Promise.race([saves.current, new Promise<void>((done) => { timer = window.setTimeout(done, 2_000); })]);
    window.clearTimeout(timer);
    await quit();
  };
  // A pick being saved already reads as remembered; only one that stayed held says so.
  const source: FormatSource = held !== null && held !== baseFormat ? saving ? "remembered" : "held"
    : writingProject || remembered ? "remembered" : app && suggested ? "suggested" : "default";
  const askPaste = Boolean(rememberAppOutput && destination?.app_id && destination.paste_question);
  const pasteChoices = Boolean(rememberAppOutput && destination?.can_choose_paste);
  const shouldAsk = askPaste || Boolean(!writingProject && rememberAppOutput && destination?.app_id && destination.output_question && !remembered && held === null);
  return { appId: destination?.app_id ?? null, app, format, ready, saving, source, pick, shouldAsk, read, choose,
    writingProject, readProblem, contextVersion: destination?.context_version ?? null,
    chooseOnce, copyToken, isCurrentCopy, consumeHeld, askPaste, pasteChoices, choosePaste, quitAfterSaving };
}
