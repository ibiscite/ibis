import { useRef, type ComponentPropsWithRef } from "react";
import { Button } from "./Button";
import "./FileDrag.css";

/** A file represented by an icon. The caller starts the native drag; no path
 * or file data enters the DOM. Pressing offers the keyboard alternative. */
export function FileDrag({ icon, label, description, onDrag, onPress }: {
  icon: string; label: string; description: string; onDrag: () => void; onPress: () => void;
}) {
  const dragged = useRef(false);
  return (
    <Button variant="icon" className="ui-file-drag" aria-label={label} title={label}
      onPress={(event) => { if (!dragged.current || event.pointerType === "keyboard" || event.pointerType === "virtual") onPress(); }}
      render={(props) => {
        const native = props as ComponentPropsWithRef<"button">;
        return <button {...native} draggable aria-description={description}
          onPointerDownCapture={(event) => { dragged.current = false; native.onPointerDownCapture?.(event); }}
          onClickCapture={(event) => {
            native.onClickCapture?.(event);
            // Suppress a released drag's mouse click, keeping keyboard and
            // assistive-technology clicks (detail 0) available.
            if (dragged.current && event.detail > 0) { event.preventDefault(); event.stopPropagation(); }
          }}
          onDragStart={(event) => {
            native.onDragStart?.(event);
            // WebKit would otherwise drag the image or HTML, not the native file URL.
            event.preventDefault();
            dragged.current = true;
            onDrag();
          }} />;
      }}>
      <img src={icon} alt="" draggable={false} />
    </Button>
  );
}
