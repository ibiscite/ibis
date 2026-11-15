// The engine's format answer and the picker's explicit format choices.
// The picker session owns the destination; this hook connects
// its native calls and says what the person picked.
import { useLayoutEffect, useRef } from "react";
import type * as nativeApi from "../../api";
import type { AppPaste } from "../../api";
import { listedOutput, type OutputId } from "../../format/outputs";
import { asksPaste, destinationFormat, destinationReady, writingVersion, sameOutput, shouldAsk, type OutputToken } from "./destination";
import { say } from "../../i18n";
import type { Operation } from "./Notices";
import type { SessionState } from "./session";
import type { createSessionController } from "./sessionController";

type DestinationApi = Partial<Pick<typeof nativeApi, "pickerDestination" | "rememberAppOutput" | "editWriting" | "forgetAppOutput">>;
/** Why the next paste uses its format. */
export type FormatSource = "remembered" | "suggested" | "default" | "held";
/** Where a format pick goes: this app, the default, or only the next paste. */
export type FormatPick = "app" | "default" | "paste";

export function useDestination(session: ReturnType<typeof createSessionController>, state: SessionState, api: DestinationApi, { fallback, chooseDefault, report }: {
  fallback: OutputId;
  chooseDefault: (choice: OutputId) => void | Promise<void>;
  report: (operation: Operation, failed?: boolean) => void;
}) {
  const { pickerDestination, rememberAppOutput, editWriting: saveWriting, forgetAppOutput } = api;
  useLayoutEffect(() => {
    session.connect({ pickerDestination, rememberAppOutput, editWriting: saveWriting, report, chooseDefault });
    session.dispatch({ type: "destination-reader", present: Boolean(pickerDestination), remembers: Boolean(rememberAppOutput) });
  });
  const d = state.destination;
  const { destination, held, saves } = d;
  const ready = destinationReady(d);
  // Whether Enter pastes into the app: unknown, so no, while it is being read.
  const canPaste = d.ready && Boolean(destination?.can_paste);
  const saving = saves > 0;
  const app = rememberAppOutput ? destination?.app ?? null : null;
  // Ids the engine no longer lists are as good as none.
  const writing = destination?.writing ?? null;
  const baseFormat = listedOutput(destination?.format.id) ?? fallback;
  const format = destinationFormat(d, fallback);
  const unlisted = Boolean(d.ready && destination && !listedOutput(destination.format.id));
  // Subscriptions keep the first render's functions, so they read the newest default.
  const defaultFormat = useRef(fallback);
  useLayoutEffect(() => {
    if (defaultFormat.current !== fallback && pickerDestination && state.destination.held === null) session.dispatch({ type: "read-destination", reset: false });
    defaultFormat.current = fallback;
  }, [fallback, pickerDestination, session, state.destination.held]);
  const current = () => session.getSnapshot().destination;
  const read = (reset = true, preserveOutput = false) => {
    if (pickerDestination) session.dispatch({ type: "read-destination", reset, preserve: preserveOutput, format: destinationFormat(current(), defaultFormat.current) });
  };
  /** Activation retires the old intent at its boundary. A read completing in
   * the same generation only supplies the preview a fresh Enter waits for. */
  const writingChanged = (version: [number, number] | null) => session.dispatch({ type: "project-changed", version });
  const editWriting = saveWriting ? session.editWriting : undefined;
  // An app that could not be read may still be a real one, so a pick cannot change the default.
  const pick: FormatPick = app ? "app" : !pickerDestination || destination ? "default" : "paste";
  const canChooseFormat = pick === "default" && held !== null || ready && !saving;
  const choose = (choice: OutputId) => {
    if (writing && destination?.writing_context && editWriting) {
      void editWriting(destination.writing_context, { kind: "output", id: writing.id, output: choice })
        .then(() => report("appChoice", false), () => report("appChoice"));
      return;
    }
    session.dispatch({ type: "choose-format", choice, pick });
  };
  /** This citation's format, and only this one: held, never saved, consumed by the copy. */
  const chooseOnce = (choice: OutputId) => session.dispatch({ type: "choose-once", choice });
  const choosePaste = (paste: AppPaste, _asked = false) => {
    if (!rememberAppOutput) return;
    session.dispatch({ type: "choose-paste", paste, output: null });
  };
  const copyToken = (): OutputToken => current().output;
  const isCurrentCopy = (token: OutputToken) => sameOutput(token, current().output);
  // A pick being saved already reads as remembered; only one that stayed held says so.
  const source: FormatSource = held !== null && held !== baseFormat ? pick === "default" ? "default" : saving ? "remembered" : "held"
    : destination?.format.from === "app" || destination?.format.from === "writing" ? "remembered"
    : destination?.format.from === "suggestion" ? "suggested" : "default";
  const useUsual = writing && destination?.writing_context && editWriting
    ? () => { void editWriting(destination.writing_context!, { kind: "select", id: null }).catch(() => report("appChoice")); }
    : destination?.app_id && forgetAppOutput ? () => {
      void session.editDestination(() => forgetAppOutput(destination.app_id!, destination.app_identity)).catch(() => report("appChoice"));
    } : undefined;
  const remembering = { ...d, remembers: Boolean(rememberAppOutput) };
  const askPaste = asksPaste(remembering);
  const pasteChoices = Boolean(rememberAppOutput && destination?.can_choose_paste);
  return { appId: destination?.app_id ?? null, app, format, ready, saving, canChooseFormat, source, pick, shouldAsk: shouldAsk(remembering), read, choose,
    findOpen: Boolean(ready && destination?.find_open),
    formatLabel: unlisted ? say("citations-unlisted") : undefined,
    writing, writingContext: destination?.writing_context ?? null, readProblem: d.editProblem ?? d.readProblem ?? (unlisted ? say("writing-unsupported") : null), contextVersion: destination?.context_version ?? null, writingChanged, editWriting,
    usualFormat: listedOutput(destination?.usual_format), useUsual,
    writingVersion: () => writingVersion(current()),
    chooseOnce, copyToken, isCurrentCopy, canPaste, askPaste, pasteChoices, choosePaste };
}
