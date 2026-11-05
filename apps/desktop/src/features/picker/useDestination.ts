// The app's remembered format → the app table's suggestion → the default.
import { useCallback, useRef, useState } from "react";
import type * as nativeApi from "../../api";
import type { AppPaste, PickerDestination } from "../../api";
import { listedOutput, type OutputId } from "../../format/outputs";
type CopyToken = { opening: number; choice: number; delivery: number };

import type { Operation } from "./Notices";

type DestinationApi = Partial<Pick<typeof nativeApi, "pickerDestination" | "rememberAppOutput" | "editWritingProject">>;
/** Why the next paste uses its format. */
export type FormatSource = "remembered" | "suggested" | "default" | "held";
/** Where a format pick goes: this app, the default, or only the next paste. */
export type FormatPick = "app" | "default" | "paste";

export function useDestination({ pickerDestination, rememberAppOutput, editWritingProject }: DestinationApi, { fallback, chooseDefault, report, onProjectContext }: {
  fallback: OutputId;
  chooseDefault: (choice: OutputId) => void;
  report: (operation: Operation, failed?: boolean) => void;
  onProjectContext?: (context: { output: CopyToken; version: string | null; clearReplacement: boolean }) => void;
}) {
  const [destination, setDestination] = useState<PickerDestination | null>(null);
  const [editProblem, setEditProblem] = useState<string | null>(null);
  // Beats the app's format until saved or copied: a pick being saved, a pick
  // that could not be saved, or the output a permission return prepared.
  const [held, setHeld] = useState<OutputId | null>(null);
  const [ready, setReady] = useState(!pickerDestination);
  const [saving, setSaving] = useState(false);
  const [readProblem, setReadProblem] = useState<string | null>(null);
  const reads = useRef(0);
  const openings = useRef(0);
  // Settings refreshes can supersede the return's read within this opening.
  const preserveOpeningOutput = useRef(false);
  const capturedId = useRef<string | null | undefined>(undefined);
  const observedVersion = useRef<[number, number] | null | undefined>(undefined);
  const settlingOpening = useRef(false);
  const settlingProject = useRef(false);
  const choices = useRef(0);
  const delivery = useRef(0);
  const lastPolicy = useRef<string | undefined>(undefined);
  const preparedFormat = useRef<OutputId>(fallback);
  const saves = useRef(Promise.resolve());
  const observer = useRef(onProjectContext);
  observer.current = onProjectContext;
  const notifyContext = useCallback((clearReplacement: boolean) => {
    observer.current?.({ output: { opening: openings.current, choice: choices.current, delivery: delivery.current },
      version: observedVersion.current?.join(":") ?? null, clearReplacement });
  }, []);
  const read = useCallback((reset = true, preserveOutput = false) => {
    if (!pickerDestination) return;
    const request = ++reads.current;
    if (reset) {
      setEditProblem(null);
      settlingOpening.current = true;
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
      const settling = settlingOpening.current || settlingProject.current;
      if (!settling && !reset && !preserveOpeningOutput.current && capturedId.current !== undefined && capturedId.current !== next.app_id) {
        openings.current += 1;
        setHeld(null);
      }
      capturedId.current = next.app_id;
      const generation = next.context_version?.[0] ?? null;
      const versionChanged = (observedVersion.current?.join(":") ?? null) !== (next.context_version?.join(":") ?? null);
      const generationChanged = observedVersion.current !== undefined && (observedVersion.current?.[0] ?? null) !== generation;
      if (generationChanged) {
        if (!settling) choices.current += 1;
        setHeld(null);
        preserveOpeningOutput.current = false;
      }
      observedVersion.current = next.context_version ?? null;
      const policy = JSON.stringify([next.app_id, next.app_identity, next.paste, next.can_paste, next.unreadable]);
      if (!reset && lastPolicy.current !== undefined && lastPolicy.current !== policy) delivery.current += 1;
      lastPolicy.current = policy;
      if (versionChanged) notifyContext(generationChanged);
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
    }).finally(() => { if (request === reads.current) {
      settlingOpening.current = false;
      settlingProject.current = false;
      setReady(true);
    } });
  }, [pickerDestination, report, notifyContext]);
  /** Activation retires the old intent at its boundary. A read completing in
   * the same generation only supplies the preview a fresh Enter waits for. */
  const projectChanged = useCallback((version: [number, number] | null) => {
    // Native generations are wrapping u32s; a late event cannot retire newer intent.
    const previous = observedVersion.current;
    if (version) {
      const part = previous && version[0] === previous[0] ? 1 : 0;
      if (previous && ((version[part] - previous[part]) >>> 0) > 0x7fff_ffff) return;
      const generationChanged = previous !== undefined && (previous?.[0] ?? null) !== version[0];
      if (generationChanged) {
        if (!settlingOpening.current && !settlingProject.current) choices.current += 1;
        preserveOpeningOutput.current = false;
        setHeld(null);
      }
      observedVersion.current = version;
      if (!previous || previous[0] !== version[0] || previous[1] !== version[1]) notifyContext(generationChanged);
    }
    read(false);
  }, [read, notifyContext]);
  const app = rememberAppOutput ? destination?.app ?? null : null;
  // Ids the engine no longer lists are as good as none.
  const remembered = listedOutput(destination?.remembered);
  // Rust works out the app table's suggestion, a formatted app's style included.
  const suggested = listedOutput(destination?.automatic);
  const writingProject = destination?.writing_project ?? null;
  const projectFormat = listedOutput(writingProject?.output);
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
  const remember = (choice: OutputId) => {
    const id = destination?.app_id;
    if (!rememberAppOutput || !id) return;
    const output = choice;
    save(() => rememberAppOutput(id, output, null, destination?.app_identity ?? null), () => {
      setDestination((old) => old && { ...old, remembered: output });
      if (destination?.unreadable) read(false);
    });
  };
  /** The sheet and the format menu share one mutation owner. Retire old work
   * before the native save, then keep new Enter waiting until its read settles. */
  const editProject = editWritingProject ? (context: string, action: Parameters<typeof editWritingProject>[1]) => {
    reads.current += 1;
    choices.current += 1;
    const opening = openings.current, choice = choices.current;
    setEditProblem(null);
    settlingProject.current = true;
    notifyContext(true);
    preserveOpeningOutput.current = false;
    setHeld(null);
    setReady(false);
    setSaving(true);
    const pending = saves.current.then(() => editWritingProject(context, action)).then((view) => {
      read(false);
      return view;
    }, (error: unknown) => {
      if (opening === openings.current && choice === choices.current && typeof error === "string") setEditProblem(error);
      read(false);
      throw error;
    });
    const settled = pending.then(() => undefined, () => undefined);
    saves.current = settled;
    void settled.then(() => { if (saves.current === settled) setSaving(false); });
    return pending;
  } : undefined;
  // An app that could not be read may still be a real one, so a pick cannot change the default.
  const pick: FormatPick = app ? "app" : !pickerDestination || destination ? "default" : "paste";
  const choose = (choice: OutputId) => {
    if (writingProject && destination?.writing_context && editProject) {
      const context = destination.writing_context;
      void editProject(context, { kind: "output", id: writingProject.id, revision: writingProject.revision, output: choice })
        .then(() => report("appChoice", false), () => report("appChoice"));
      return;
    }
    choices.current += 1;
    setHeld(choice);
    if (!ready) return;
    if (pick === "app") remember(choice);
    else if (pick === "default") { chooseDefault(choice); setHeld(null); }
  };
  /** This citation's format, and only this one: held, never saved, consumed by the copy. */
  const chooseOnce = (choice: OutputId) => {
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
    const output = asked && !writingProject && held === null && destination?.output_question && !remembered ? format : null;
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
    writingProject, writingContext: destination?.writing_context ?? null, readProblem: editProblem ?? readProblem, contextVersion: destination?.context_version ?? null, projectChanged, editProject,
    projectVersion: () => observedVersion.current?.join(":") ?? null,
    chooseOnce, copyToken, isCurrentCopy, consumeHeld, askPaste, pasteChoices, choosePaste, quitAfterSaving };
}
