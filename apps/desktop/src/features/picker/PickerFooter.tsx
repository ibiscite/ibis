import type React from "react";
import { useId, useLayoutEffect, useRef, useState } from "react";
import type { LibraryStatus } from "../../api";
import { keysFor } from "../../keys";
import { LibraryStatusLine } from "../../LibraryStatusLine";
import "../../ui/Footer.css";
import { say } from "../../i18n";
import { currentPlatform } from "../../platform";
import { Button } from "../../ui/Button";
import { QuietStatus } from "../../ui/Delights";
import { FrameCitationSlot } from "../../ui/Frame";
import { Icon } from "../../icons";
import { Status } from "../../ui/Status";

export type KeyHints = { enter: string | null; shiftEnter: string | null; escape: string };

/** The delivery bar: the citation (its tokens, filled in by the citation line),
 * the library when it needs the person, the one action, then More. More's place
 * never depends on the action: shorten the status, then truncate the label, then
 * fold labels to keys by width. A refusal turns the action off and says why on a
 * line of the bar's own, with its fixes; Enter then copies nothing. */
export function PickerFooter({ library, groupName, hints, altHeld, actions, more, status, emptyAction, refusal, refusalActions, refused }: {
  library: LibraryStatus | null | undefined; groupName: (id: number) => string; hints: KeyHints; altHeld: boolean;
  actions: { copy: () => void; add: () => void; escape: () => void };
  more: React.ReactNode; status?: React.ReactNode; emptyAction?: boolean;
  /** Why the citation cannot be delivered, in plain words; the action is off while it stands. */
  refusal?: string | null;
  /** The refusal's fixes, beside its words. */
  refusalActions?: React.ReactNode;
  /** Enter was pressed on the refused citation: said again for screen readers. */
  refused?: boolean;
}) {
  const reasonId = useId();
  const bar = useRef<HTMLElement>(null);
  const hintsRow = useRef<HTMLElement>(null);
  const menu = useRef<HTMLSpanElement>(null);
  const statusSize = useRef<HTMLSpanElement>(null);
  const statusMinimum = useRef<HTMLSpanElement>(null);
  const labelMinimum = useRef<HTMLSpanElement>(null);
  const citationMinimum = useRef<HTMLSpanElement>(null);
  const [fit, setFit] = useState({ status: false, labels: false, alt: false });
  const platform = currentPlatform();
  // biome-ignore lint/correctness/useExhaustiveDependencies: labels, Alt and platform change intrinsic control widths.
  useLayoutEffect(() => {
    const element = bar.current;
    if (!element) return;
    const measure = () => {
      const width = (node: Element | null) => node?.getBoundingClientRect().width ?? 0;
      // The citation keeps room for a token or two; the rest folds around it.
      const available = element.clientWidth - (element.querySelector(".bar-citation > .shelf") ? width(citationMinimum.current) : 0);
      const preferred = width(statusSize.current);
      if (!(available > 0 && preferred > 0)) return;
      const gap = parseFloat(getComputedStyle(element).columnGap) || 0;
      const intrinsic = (button: Element | null) => {
        const label = button?.querySelector<HTMLElement>(".key-hint-label");
        if (!label) return width(button);
        const labelGap = label.parentElement ? parseFloat(getComputedStyle(label.parentElement).columnGap) || 0 : 0;
        // A truncated button is narrower than its name; fit by what it wants.
        return width(button) + (getComputedStyle(label).position === "absolute" ? label.scrollWidth + labelGap : Math.max(0, label.scrollWidth - label.clientWidth));
      };
      // The fix's own wish, never what the action leaves it: it reserves its
      // room in the leading column, so it can neither wrap onto the action nor fold it.
      const fix = intrinsic(element.querySelector(".source-action"));
      element.style.setProperty("--footer-fix", `${fix}px`);
      const actions = [...(hintsRow.current?.querySelectorAll(".footer-action") ?? [])];
      const hintGap = hintsRow.current ? parseFloat(getComputedStyle(hintsRow.current).columnGap) || 0 : 0;
      // More, and the pin beside it while the window is kept open.
      const moreWidth = intrinsic(menu.current?.querySelector(".more-button") ?? menu.current?.querySelector("button") ?? null) + width(menu.current?.querySelector(".footer-pinned") ?? null);
      const status = preferred + fix + moreWidth + actions.reduce((sum, action) => sum + intrinsic(action), 0)
        + Math.max(0, actions.length - 1) * hintGap + gap * 2 > available;
      // Never the action's own width: its label must not be able to resize More.
      const labels = width(statusMinimum.current) + fix + moreWidth + width(labelMinimum.current) + gap * 2 > available;
      // Alt's two actions cannot truncate both into one line: they keep their
      // keys and fold their labels, never More's, so More keeps its size.
      const alt = altHeld && width(statusMinimum.current) + fix + moreWidth + gap * 2
        + actions.reduce((sum, action) => sum + intrinsic(action), 0) + Math.max(0, actions.length - 1) * hintGap > available;
      setFit((old) => old.status === status && old.labels === labels && old.alt === alt ? old : { status, labels, alt });
    };
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    for (const node of [bar.current, statusSize.current, hintsRow.current, menu.current, bar.current?.querySelector(".source-action")]) if (node) observer?.observe(node);
    window.addEventListener("resize", measure);
    return () => { observer?.disconnect(); window.removeEventListener("resize", measure); };
  }, [hints.enter, hints.shiftEnter, hints.escape, altHeld, platform, status]);
  return <footer className="delivery-bar" data-menu-floor data-refused={refusal ? "" : undefined}>
    {/* The reason sits in the bar, over the citation it is about, with its fixes. */}
    {refusal && <div className="bar-reason">
      <span className="bar-reason-mark" aria-hidden="true"><Icon name="warning" /></span>
      <Status id={reasonId} className="bar-reason-words" dir="auto" role="alert">{refusal}</Status>
      {refusalActions}
    </div>}
    {/* The reason is an alert as it arrives; Enter meeting it is said here, mounted with the reason so it is heard. */}
    {refusal && <Status className="visually-hidden" role="alert">{refused ? say("picker-bar-refused", { reason: refusal }) : ""}</Status>}
    {/* The region includes More and Copy. Its name must stay distinct from
        the Library button, so native name lookup cannot activate the region. */}
    <section ref={bar} aria-label={say("picker-picker-actions")} className="library-bar" data-empty-action={emptyAction || undefined}
      data-status-compact={fit.status || undefined} data-hints-compact={fit.labels || undefined} data-alt-compact={altHeld && fit.alt || undefined}>
      <span className="footer-fit-measure" aria-hidden="true" inert><span ref={statusSize} className="footer-status-size" /><span ref={statusMinimum} className="footer-status-minimum" /><span ref={labelMinimum} className="footer-label-minimum" /><span ref={citationMinimum} className="footer-citation-minimum" /></span>
      <FrameCitationSlot />
      {status ?? <LibraryStatusLine library={library} groupName={groupName} />}
      <small key="hints" ref={hintsRow} className="enter-hints" aria-label={say("key-footer")} role="group">
        {altHeld ? <>
          {hints.shiftEnter && <FooterAction label={hints.shiftEnter} keys={keysFor("add")} onAction={actions.add} />}
          <FooterAction label={hints.escape} keys={keysFor("escape")} onAction={actions.escape} />
        </> : hints.enter ? <span className="footer-primary"><FooterAction label={hints.enter} keys={keysFor("copy")} primary onAction={actions.copy} unavailable={refusal ? reasonId : undefined} /></span>
          : emptyAction && <FooterAction label={say("key-copy")} keys={keysFor("copy")} disabled onAction={actions.copy} />}
      </small>
      <span key="more" ref={menu} className="more">{more}</span>
      <QuietStatus>{library?.note}</QuietStatus>
    </section>
  </footer>;
}

/** `unavailable` names the reason the action is off: it stays focusable and
 * spoken, its words say why, and a press only says why again. */
function FooterAction({ label, keys, primary, onAction, disabled, unavailable }: { label: string; keys: string; primary?: boolean; onAction: () => void; disabled?: boolean; unavailable?: string }) {
  return <Button variant={primary ? "action" : "quiet"} size="small" className="footer-action" label={label} keys={keys} title={label} isDisabled={disabled}
    aria-disabled={unavailable ? true : undefined} aria-describedby={unavailable} onPress={onAction} />;
}
