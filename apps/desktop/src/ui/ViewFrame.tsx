import { useLayoutEffect, useRef, type HTMLAttributes, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import { Button } from "./Button";
import { IconButton } from "./IconButton";
import { KeyHint } from "./KeyCaps";
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
 * view's name, then an optional search field; the body; and a footer whose status
 * says where the choice applies, with the view's actions and Back with Esc. One
 * frame for the format and actions palette and every sheet. */
export function ViewFrame({ title, titleId, search, back, status, actions, leadingActions, children, frameRef, ...dialog }: {
  /** The view's name: a quiet chip before its search ("Format"), or plain title text ("Cited Since Opening"). */
  title: string; titleId: string;
  /** The view's own search field, after its name. */
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
  useLayoutEffect(() => {
    const view = frame.current!;
    views.add(view); sync();
    return () => leaveView(view);
  }, [frame]);
  return createPortal(<div ref={frame} className="ui-view" aria-labelledby={titleId} {...dialog}>
    <div className="ui-view-head">
      {/* The pointer's Back; the footer's Back is the one keys and screen readers reach. */}
      <IconButton icon="back" size="large" className="ui-view-back" aria-label={back.label} title={back.label} aria-hidden excludeFromTabOrder onPress={back.onBack} />
      {/* A chip says "searching within"; a view with no search is named by plain title text. */}
      <h2 id={titleId} className="ui-view-title" data-chip={search ? "" : undefined}>{title}</h2>
      {search}
    </div>
    <div className="ui-view-body">{children}</div>
    <div className="ui-view-foot">
      <span className="ui-view-status">{status}{leadingActions}</span>
      {actions}
      <Button variant="quiet" size="small" className="ui-view-back-hint" aria-keyshortcuts="Escape" onPress={back.onBack}>
        <KeyHint label={back.label} keys="Escape" hidden />
      </Button>
    </div>
  </div>, document.body);
}
