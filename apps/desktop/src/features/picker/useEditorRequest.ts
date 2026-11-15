import type * as nativeApi from "../../api";
// An editor waiting on the endpoint. Native code owns the request; the picker
// shows that one is waiting and asks for a fresh preview, which native code
// writes in the editor's format.
import { useCallback, useEffect, useRef, useState } from "react";
import { useNativeEvent } from "../../nativeEvents";

export function useEditorRequest(api: typeof nativeApi) {
  const { editorRequest, onEditorRequest } = api;
  const [label, setLabel] = useState<string | null>(null);
  const [style, setStyle] = useState(false);
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
      setStyle(next?.style ?? false);
      setVersion((old) => old + 1);
    }, () => {});
  }, [editorRequest]);
  useEffect(() => {
    read();
    return () => { asked.current += 1; };
  }, [read]);
  // A request that began before this listener existed sent its event to nobody.
  useNativeEvent(onEditorRequest, read, { ready: read });
  return { label, version, style };
}
