// The app's remembered format → the app table's suggestion → the default.
// The picker session owns the destination (decision 0035); this hook connects
// its native calls and says what the person picked.
import { useLayoutEffect, useRef } from "react";
import type * as nativeApi from "../../api";
import type { AppPaste } from "../../api";
import { listedOutput, type OutputId } from "../../format/outputs";
import { destinationFormat, projectVersion, sameOutput, type OutputToken, type ProjectEdit } from "./destination";
import type { Operation } from "./Notices";
import type { SessionState } from "./session";
import type { createSessionController } from "./sessionController";

type DestinationApi = Partial<Pick<typeof nativeApi, "pickerDestination" | "rememberAppOutput" | "editWritingProject">>;
/** Why the next paste uses its format. */
export type FormatSource = "remembered" | "suggested" | "default" | "held";
/** Where a format pick goes: this app, the default, or only the next paste. */
export type FormatPick = "app" | "default" | "paste";

export function useDestination(session: ReturnType<typeof createSessionController>, state: SessionState, api: DestinationApi, { fallback, chooseDefault, report }: {
  fallback: OutputId;
  chooseDefault: (choice: OutputId) => void;
  report: (operation: Operation, failed?: boolean) => void;
}) {
  const { pickerDestination, rememberAppOutput, editWritingProject } = api;
  useLayoutEffect(() => {
    session.connect({ pickerDestination, rememberAppOutput, editWritingProject, report, chooseDefault });
    session.dispatch({ type: "destination-reader", present: Boolean(pickerDestination) });
  });
  const d = state.destination;
  const { destination, held, saves } = d;
  const ready = !pickerDestination || d.ready;
  // Whether Enter pastes into the app: unknown, so no, while it is being read.
  const canPaste = d.ready && Boolean(destination?.can_paste);
  const saving = saves > 0;
  const app = rememberAppOutput ? destination?.app ?? null : null;
  // Ids the engine no longer lists are as good as none.
  const remembered = listedOutput(destination?.remembered);
  // Rust works out the app table's suggestion, a formatted app's style included.
  const suggested = listedOutput(destination?.automatic);
  const writingProject = destination?.writing_project ?? null;
  const baseFormat = listedOutput(writingProject?.output) ?? remembered ?? suggested ?? fallback;
  const format = destinationFormat(d, fallback);
  // Subscriptions keep the first render's functions, so they read the newest default.
  const defaultFormat = useRef(fallback);
  defaultFormat.current = fallback;
  const current = () => session.getSnapshot().destination;
  const read = (reset = true, preserveOutput = false) => {
    if (pickerDestination) session.dispatch({ type: "read-destination", reset, preserve: preserveOutput, format: destinationFormat(current(), defaultFormat.current) });
  };
  /** Activation retires the old intent at its boundary. A read completing in
   * the same generation only supplies the preview a fresh Enter waits for. */
  const projectChanged = (version: [number, number] | null) => session.dispatch({ type: "project-changed", version });
  const editProject = editWritingProject ? (context: string, action: ProjectEdit) => session.editProject(context, action) : undefined;
  // An app that could not be read may still be a real one, so a pick cannot change the default.
  const pick: FormatPick = app ? "app" : !pickerDestination || destination ? "default" : "paste";
  const choose = (choice: OutputId) => {
    if (writingProject && destination?.writing_context && editProject) {
      void editProject(destination.writing_context, { kind: "output", id: writingProject.id, output: choice })
        .then(() => report("appChoice", false), () => report("appChoice"));
      return;
    }
    session.dispatch({ type: "choose-format", choice, pick });
  };
  /** This citation's format, and only this one: held, never saved, consumed by the copy. */
  const chooseOnce = (choice: OutputId) => session.dispatch({ type: "choose-once", choice });
  const choosePaste = (paste: AppPaste, asked = false) => {
    if (!rememberAppOutput) return;
    // Answering an open first-paste question accepts the format it showed, so it is not asked again.
    const output = asked && !writingProject && held === null && destination?.output_question && !remembered ? format : null;
    session.dispatch({ type: "choose-paste", paste, output });
  };
  const copyToken = (): OutputToken => current().output;
  const isCurrentCopy = (token: OutputToken) => sameOutput(token, current().output);
  const quitAfterSaving = async (quit: () => Promise<void>) => {
    await session.afterSaves(2_000);
    await quit();
  };
  // A pick being saved already reads as remembered; only one that stayed held says so.
  const source: FormatSource = held !== null && held !== baseFormat ? saving ? "remembered" : "held"
    : writingProject || remembered ? "remembered" : app && suggested ? "suggested" : "default";
  const askPaste = Boolean(rememberAppOutput && destination?.app_id && destination.paste_question);
  const pasteChoices = Boolean(rememberAppOutput && destination?.can_choose_paste);
  const shouldAsk = askPaste || Boolean(!writingProject && rememberAppOutput && destination?.app_id && destination.output_question && !remembered && held === null);
  return { appId: destination?.app_id ?? null, app, format, ready, saving, source, pick, shouldAsk, read, choose,
    writingProject, writingContext: destination?.writing_context ?? null, readProblem: d.editProblem ?? d.readProblem, contextVersion: destination?.context_version ?? null, projectChanged, editProject,
    projectVersion: () => projectVersion(current()),
    chooseOnce, copyToken, isCurrentCopy, canPaste, askPaste, pasteChoices, choosePaste, quitAfterSaving };
}
