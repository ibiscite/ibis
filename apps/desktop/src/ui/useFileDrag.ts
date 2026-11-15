import { useEffect, useRef, useState, type DragEvent } from "react";
import { useNativeEvent, type Source } from "../nativeEvents";
import "./FileDrop.css";

/** Only drag presence enters the UI; native-owned paths never do. */
export function useFileDrag(subscribe?: Source<(active: boolean) => void>, presentation?: number) {
  const [active, setActive] = useState(false);
  const depth = useRef(0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new presentation retires its drag.
  useEffect(() => { depth.current = 0; setActive(false); }, [presentation]);
  // A drag reported to an earlier presentation cannot mark this one.
  useNativeEvent(subscribe, (dragging) => { if (!dragging) depth.current = 0; setActive(dragging); }, { key: presentation });
  const clear = () => { depth.current = 0; setActive(false); };
  return {
    "data-file-drag": active || undefined,
    onDragEnter: (event: DragEvent) => { if ([...event.dataTransfer.types].includes("Files")) { depth.current += 1; setActive(true); } },
    onDragLeave: () => { depth.current = Math.max(0, depth.current - 1); if (!depth.current) setActive(false); },
    onDrop: clear,
  };
}
