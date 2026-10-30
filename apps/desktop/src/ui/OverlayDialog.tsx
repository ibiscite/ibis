import { useRef, type ReactNode } from "react";
import { Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";
import { composing } from "../composition";
import { keyText } from "../messages";
import { Button } from "./Button";
import { useScrollEdges } from "./ScrollArea";
import "./Menu.css";
import "./OverlayDialog.css";

/** On-demand details use the shared overlay surface and one bounded scroller.
 * React Aria owns modality, Tab containment and return focus. */
export function OverlayDialog({ title, heading = title, children, onClose }: { title: string; heading?: string; children: ReactNode; onClose: () => void }) {
  const scroll = useRef<HTMLDivElement>(null);
  useScrollEdges(scroll);
  return <ModalOverlay className="overlay-backdrop" isOpen isDismissable onOpenChange={(open) => { if (!open) onClose(); }}>
    <Modal className="menu-popover overlay-panel">
      <div className="overlay-event-scope" onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Escape" && !composing(event)) { event.preventDefault(); onClose(); }
      }}>
      <Dialog className="overlay-dialog" aria-label={title}>
        <header className="overlay-heading"><Heading slot="title" title={heading}>{heading}</Heading>
          <Button variant="icon" size="small" glyph="close" aria-label={keyText.close} onPress={onClose} />
        </header>
        <div ref={scroll} className="overlay-scroll scroll-area">{children}</div>
      </Dialog>
      </div>
    </Modal>
  </ModalOverlay>;
}
