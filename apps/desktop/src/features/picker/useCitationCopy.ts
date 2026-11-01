import { useLayoutEffect, type RefObject } from "react";
import type { PickerApi } from "../../App";
import type { SessionState, SessionView } from "./session";
import type { createSessionController } from "./sessionController";
import type { useDestination } from "./useDestination";
import type { useOperations } from "./useOperations";
import type { Choice } from "./choice";
import type { MenuCloseReason } from "../../ui/ToolbarMenu";

export function useCitationCopy({ api, session, state, destination, currentPin, formatButtonRef, refreshSnapshot, report, view, setFormatOpen }: {
  api: PickerApi;
  session: ReturnType<typeof createSessionController>;
  state: SessionState;
  destination: ReturnType<typeof useDestination>;
  currentPin: RefObject<boolean>;
  formatButtonRef: RefObject<HTMLButtonElement | null>;
  refreshSnapshot: () => void;
  report: ReturnType<typeof useOperations>["report"];
  view: Omit<SessionView, "appId" | "ready" | "saving" | "shouldAsk" | "output"> & { ready: boolean };
  setFormatOpen: (open: boolean) => void;
}) {
  const { copyFileCitation, hidePicker, showCopiedNotice } = api;
  const send = session.dispatch;
  const { queryKey, format, snapshotRevision, defaultChoice, chosen, shown, previewVisible: showPastePreview, searchPending } = view;
  // The controller receives readiness from the existing search/preview I/O
  // adapters. Session and interaction tickets reject an obsolete render.
  useLayoutEffect(() => {
    session.connect({ copyFileCitation, hidePicker, pickerPresentation: api.pickerPresentation, showCopiedNotice, output: destination.copyToken,
      pinned: () => currentPin.current, consume: destination.consumeOnce, refresh: refreshSnapshot,
      askFormat: () => formatButtonRef.current?.click(), reportHide: (failed) => report("hide", failed) });
    send({ type: "view", session: state.session, interaction: state.interaction,
      view: { appId: destination.appId, queryKey, format, snapshotRevision, defaultChoice, chosen, shown, previewVisible: showPastePreview, searchPending,
        ready: destination.ready && view.ready, saving: destination.saving, shouldAsk: destination.shouldAsk,
        output: destination.copyToken() } });
  });
  const requestCopy = (choice: string | null) => send({ type: "enter", choice, output: destination.copyToken() });
  const chooseFormat = (choice: Choice) => {
    destination.choose(choice);
    send({ type: "question", format: choice, output: destination.copyToken(), answer: destination.askPaste ? "cancel" : "answered" });
  };
  const questionScope = (action: () => void, keepOpen = false) => {
    action();
    send({ type: "question", format: null, output: destination.copyToken(), answer: keepOpen ? "open" : destination.askPaste ? "cancel" : "answered" });
    if (keepOpen && session.getSnapshot().pendingCopy?.question === "open") formatButtonRef.current?.click();
  };
  const onFormatOpenChange = (open: boolean, reason?: MenuCloseReason) => {
    setFormatOpen(open);
    if (open || state.pendingCopy?.question !== "open" || reason === "action") return;
    // Dismissal uses the fallback for this attempt; application commands cancel it.
    send({ type: "question", answer: destination.askPaste || reason === "command" ? "cancel" : "answered" });
  };
  return { requestCopy, chooseFormat, questionScope, onFormatOpenChange };
}
