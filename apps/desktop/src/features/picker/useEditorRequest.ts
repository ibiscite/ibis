// An editor waiting on the endpoint. Native code owns the request; the picker
// shows that one is waiting and asks for a fresh preview, which native code
// writes in the editor's format.
import { useCallback, useEffect, useRef, useState } from "react";
import type { PickerApi } from "../../App";

export function useEditorRequest(api: PickerApi) {
  const { editorRequest, onEditorRequest } = api;
  const [label, setLabel] = useState<string | null>(null);
  const [version, setVersion] = useState(0);
  const asked = useRef(0);
  const shown = useRef<number | null>(null);
  const read = useCallback(() => {
    const ticket = ++asked.current;
    void editorRequest?.().then((next) => {
      if (ticket !== asked.current) return;
      // Each request and each end changes what may be copied, so each asks again;
      // a picker with no editor waiting asks nothing.
      const request = next?.request ?? null;
      if (request === shown.current) return;
      shown.current = request;
      setLabel(next?.format ?? null);
      setVersion((old) => old + 1);
    }, () => {});
  }, [editorRequest]);
  useEffect(() => {
    read();
    const unlisten = onEditorRequest?.(read);
    void unlisten?.then(read);
    return () => {
      asked.current += 1;
      void unlisten?.then((stop) => stop());
    };
  }, [read, onEditorRequest]);
  return { label, version };
}
