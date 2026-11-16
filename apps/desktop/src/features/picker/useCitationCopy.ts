import type * as nativeApi from "../../api";
import { useLayoutEffect, type RefObject } from "react";
import type { SessionState } from "./session";
import type { createSessionController } from "./sessionController";
import type { useDestination } from "./useDestination";
import type { OutputId } from "../../format/outputs";
import type { MenuCloseReason } from "./FormatMenu";

export function useCitationCopy({ api, session, state, destination, formatButtonRef, refreshSnapshot, setFormatOpen }: {
  api: typeof nativeApi;
  session: ReturnType<typeof createSessionController>;
  state: SessionState;
  destination: ReturnType<typeof useDestination>;
  formatButtonRef: RefObject<HTMLButtonElement | null>;
  refreshSnapshot: () => void;
  setFormatOpen: (open: boolean) => void;
}) {
  const { deliverCitation } = api;
  const send = session.dispatch;
  useLayoutEffect(() => {
    session.connect({ deliverCitation, pickerPresentation: api.pickerPresentation,
      refresh: refreshSnapshot });
  });
  // An Enter that must first ask opens the format menu once the screen shows its question.
  const asking = state.pendingCopy?.question === "open";
  useLayoutEffect(() => { if (asking) formatButtonRef.current?.click(); }, [asking, formatButtonRef]);
  const requestCopy = (choice: string | null) => send({ type: "enter", choice, output: destination.copyToken() });
  const chooseFormat = (choice: OutputId) => {
    destination.choose(choice);
    send({ type: "question", format: choice, output: destination.copyToken(), answer: destination.askPaste ? "cancel" : "answered" });
  };
  const onFormatOpenChange = (open: boolean, reason?: MenuCloseReason) => {
    setFormatOpen(open);
    if (open || state.pendingCopy?.question !== "open" || reason === "action") return;
    // Dismissal uses the fallback for this attempt; application commands cancel it.
    send({ type: "question", answer: destination.askPaste || reason === "command" ? "cancel" : "answered" });
  };
  return { requestCopy, chooseFormat, onFormatOpenChange };
}
