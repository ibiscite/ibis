import { useState, useSyncExternalStore } from "react";
import type { PickerPreview } from "../../App";
import { initialSession } from "./session";
import { createSessionController } from "./sessionController";

export function usePickerSession(preview?: PickerPreview) {
  const [session] = useState(() => createSessionController(initialSession(preview?.query, preview?.copied)));
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot);
  const send = session.dispatch;
  const setQuery = (value: string) => send({ type: "query", value });
  const setActive = (index: number) => send({ type: "highlight", index });
  const cancelCopy = () => send({ type: "cancel" });
  const isCurrentSession = (id: number) => session.getSnapshot().session === id;
  return { session, state, send, setQuery, setActive, cancelCopy, isCurrentSession };
}
