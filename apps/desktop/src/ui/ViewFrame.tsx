import { useLayoutEffect, useRef, type HTMLAttributes, type ReactNode, type RefObject } from "react";
import { createPortal, flushSync } from "react-dom";
import { Band } from "./Band";
import { Button } from "./Button";
import { IconButton } from "./IconButton";
import { KeyHint } from "./KeyCaps";
import { composingKey, useLayer } from "./layers";
import { useBackNavigation } from "./useBackNavigation";
import "./ViewFrame.css";

/** The views open now. While any is, the picker's own content is hidden: the window shows one thing. */
const views = new Set<object>();
const sync = () => {
  if (views.size) document.documentElement.dataset.view = "";
  else delete document.documentElement.dataset.view;
};
/** This view is leaving: show what is under it now, before focus returns to something there. */
export function leaveView(view: object) { views.delete(view); sync(); }

/** A view of the picker window (maintainer, 2026-11-10: palette and sheets, option
 * B): the picker's content swaps out for the view, edge to edge, in the picker's own
 * header and footer geometry. The back chevron sits where the bird does, then the
 * search field or the view's name; the body; and a footer whose status
 * says where the choice applies, with the view's actions and Back with Esc. One
 * frame for the format and actions palette and every sheet. */
export function ViewFrame({ title, titleId, search, back, status, actions, leadingActions, children, frameRef, onKeyDown, ...dialog }: {
  /** The dialog's name; visible in the header only when there is no search. */
  title: string; titleId: string;
  /** The view's own search field, whose placeholder names the view visually. */
  search?: ReactNode;
  back: { label: string; onBack: () => void };
  /** Where the choice applies, in the footer's status place. */
  status?: ReactNode;
  /** The footer's actions: the leading ones, then the trailing ones with the primary last. */
  actions?: ReactNode; leadingActions?: ReactNode;
  children: ReactNode;
  /** The frame; also the view's identity for leaveView. */
  frameRef?: RefObject<HTMLDivElement | null>;
} & Omit<HTMLAttributes<HTMLDivElement>, "title">) {
  const own = useRef<HTMLDivElement>(null), frame = frameRef ?? own;
  // Commands can open native UI next: this view must already be gone then.
  const ownsKeys = useLayer(search ? "menu" : "sheet", true, () => { flushSync(back.onBack); });
  useBackNavigation(back.onBack, ownsKeys);
  useLayoutEffect(() => {
    const view = frame.current!;
    views.add(view); sync();
    return () => leaveView(view);
  }, [frame]);
  return createPortal(<div ref={frame} className="ui-view" aria-labelledby={titleId} {...dialog}
    onKeyDown={(event) => { if (ownsKeys() && !composingKey(event.nativeEvent)) onKeyDown?.(event); }}>
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
