// Whether Ibis starts at login, as the system reports it, and the More menu's switch for it.
import { useEffect, useRef, useState } from "react";
import type * as nativeApi from "../../api";
import type { LoginItem } from "../../api";
import type { useOperations } from "./useOperations";

export function useLoginItem({ loginItem, setLoginItem, perform, menuOpen }: {
  loginItem?: typeof nativeApi.loginItem;
  setLoginItem?: typeof nativeApi.setLoginItem;
  perform: ReturnType<typeof useOperations>["perform"];
  /** Read again each time More opens, since people also change it in system settings. */
  menuOpen: boolean;
}) {
  const [startAtLogin, setStartAtLogin] = useState<LoginItem | null>(null);
  // Calls run one after another, and only the newest one's answer is shown,
  // so quick clicks end in the state of the last click.
  const loginQueue = useRef<Promise<unknown>>(Promise.resolve());
  const loginRequest = useRef(0);
  const askLoginItem = (operation: "loginItem" | "loginItemChange", call: () => Promise<LoginItem>, failed?: () => void) => {
    const request = ++loginRequest.current;
    const run = loginQueue.current.catch(() => undefined).then(call).then(
      (item) => { if (request === loginRequest.current) setStartAtLogin(item); },
      (error: unknown) => { if (request === loginRequest.current) failed?.(); throw error; },
    );
    loginQueue.current = run;
    void perform(operation, () => run);
  };
  const readLoginItem = () => {
    if (loginItem) askLoginItem("loginItem", loginItem);
  };
  const toggleStartAtLogin = () => {
    if (!setLoginItem || !startAtLogin) return;
    const before = startAtLogin;
    const on = !before.enabled;
    // Shown at once; the system's answer replaces it.
    setStartAtLogin({ ...before, enabled: on, problem: null });
    askLoginItem("loginItemChange", () => setLoginItem(on), () => setStartAtLogin({ ...before, problem: null }));
  };
  // biome-ignore lint/correctness/useExhaustiveDependencies: reads once each time More opens; readLoginItem is rebuilt every render.
  useEffect(() => { if (menuOpen) readLoginItem(); }, [menuOpen]);
  const offered = Boolean(setLoginItem) && startAtLogin !== null && startAtLogin.problem !== "unsupported";
  return { startAtLogin, readLoginItem, toggleStartAtLogin, offered };
}
