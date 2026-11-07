import { useLayoutEffect, type RefObject } from "react";
import type { PickerApi } from "../../App";
import type { SessionState } from "./session";
import type { createSessionController } from "./sessionController";
import type { useDestination } from "./useDestination";
import type { useOperations } from "./useOperations";
import type { OutputId } from "../../format/outputs";
import type { MenuCloseReason } from "../../ui/ToolbarMenu";

export function useCitationCopy({ api, session, state, destination, currentPin, formatButtonRef, refreshSnapshot, report, setFormatOpen }: {
  api: PickerApi;
  session: ReturnType<typeof createSessionController>;
  state: SessionState;
  destination: ReturnType<typeof useDestination>;
  currentPin: RefObject<boolean>;
  formatButtonRef: RefObject<HTMLButtonElement | null>;
  refreshSnapshot: () => void;
  report: ReturnType<typeof useOperations>["report"];
  setFormatOpen: (open: boolean) => void;
}) {
  const { copyFileCitation, hidePicker, showCopiedNotice } = api;
  const send = session.dispatch;
  useLayoutEffect(() => {
    session.connect({ copyFileCitation, hidePicker, pickerPresentation: api.pickerPresentation, showCopiedNotice,
      pinned: () => currentPin.current, refresh: refreshSnapshot,
      reportHide: (failed) => report("hide", failed) });
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
