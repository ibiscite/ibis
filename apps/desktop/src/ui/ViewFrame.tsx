import type { HTMLAttributes, ReactNode, RefObject } from "react";
import { createPortal } from "react-dom";
import { Band } from "./Band";
import { Button } from "./Button";
import { IconButton } from "./IconButton";
import { KeyHint } from "./KeyCaps";
import { composingKey, type LayerHandle } from "./layers";
import "./ViewFrame.css";

/** A view of the picker window (maintainer, 2026-11-10: palette and sheets, option
 * B): the picker's content swaps out for the view, edge to edge, in the picker's own
 * header and footer geometry. The back chevron sits where the bird does, then the
 * search field or the view's name; the body; and a footer whose status
 * says where the choice applies, with the view's actions and Back with Esc. One
 * frame for the format and actions palette and every sheet. */
export function ViewFrame({ title, titleId, search, back, layer, status, actions, leadingActions, children, frameRef, onKeyDown, ...dialog }: {
  /** The dialog's name; visible in the header only when there is no search. */
  title: string; titleId: string;
  /** The view's own search field, whose placeholder names the view visually. */
  search?: ReactNode;
  back: { label: string; onBack: () => void };
  /** The view's place on the view stack (ui/layers), which its owner registers
   * with what going back means; while the window shows the view it hides the rest. */
  layer: LayerHandle;
  /** Where the choice applies, in the footer's status place. */
  status?: ReactNode;
  /** The footer's actions: the leading ones, then the trailing ones with the primary last. */
  actions?: ReactNode; leadingActions?: ReactNode;
  children: ReactNode;
  frameRef?: RefObject<HTMLDivElement | null>;
} & Omit<HTMLAttributes<HTMLDivElement>, "title">) {
  return createPortal(<div ref={frameRef} className="ui-view" aria-labelledby={titleId} {...dialog}
    onKeyDown={(event) => { if (layer.nearest() && !composingKey(event.nativeEvent)) onKeyDown?.(event); }}>
    {/* The pointer's Back, in the icon column; the footer's Back is the one keys and screen readers reach. */}
    <Band className="ui-view-head" icon={<IconButton icon="back" className="ui-view-back" aria-label={back.label} aria-hidden excludeFromTabOrder onPress={back.onBack} />}>
      {search ? <span id={titleId} className="visually-hidden">{title}</span>
        : <h2 id={titleId} className="ui-view-title">{title}</h2>}
      {search}
    </Band>
    <div className="ui-view-body">{children}</div>
    <Band className="ui-view-foot" trailing={<>
      {actions}
      <Button variant="quiet" size="small" className="ui-view-back-hint" aria-keyshortcuts="Escape" onPress={back.onBack}>
        <KeyHint label={back.label} keys="Escape" hidden />
      </Button>
    </>}>
      <span className="ui-view-status">{status}{leadingActions}</span>
    </Band>
  </div>, document.body);
}
