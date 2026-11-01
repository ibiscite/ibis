import { useEffect, useRef, useState, type DragEvent } from "react";
import "./FileDrop.css";

/** Only drag presence enters the UI; native-owned paths never do. */
export function useFileDrag(subscribe?: (change: (active: boolean) => void) => Promise<() => void>, presentation?: number) {
  const [active, setActive] = useState(false);
  const depth = useRef(0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new presentation retires its drag and native subscription.
  useEffect(() => {
    depth.current = 0; setActive(false);
    let current = true;
    const pending = subscribe?.((dragging) => { if (current) { if (!dragging) depth.current = 0; setActive(dragging); } }).catch(() => () => undefined);
    return () => { current = false; void pending?.then((stop) => stop()).catch(() => undefined); };
  }, [subscribe, presentation]);
  const clear = () => { depth.current = 0; setActive(false); };
  return {
    "data-file-drag": active || undefined,
    onDragEnter: (event: DragEvent) => { if ([...event.dataTransfer.types].includes("Files")) { depth.current += 1; setActive(true); } },
    onDragLeave: () => { depth.current = Math.max(0, depth.current - 1); if (!depth.current) setActive(false); },
    onDrop: clear,
  };
}
