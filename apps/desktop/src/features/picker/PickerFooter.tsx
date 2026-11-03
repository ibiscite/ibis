import type React from "react";
import { useLayoutEffect, useRef, useState } from "react";
import type { LibraryStatus } from "../../api";
import { keysFor } from "../../keys";
import { LibraryStatusLine } from "../../LibraryStatusLine";
import "./PickerFooter.css";
import { keyText, pickerText } from "../../messages";
import { currentPlatform } from "../../platform";
import { Button } from "../../ui/Button";
import { QuietStatus } from "../../ui/Delights";

export type KeyHints = { enter: string | null; shiftEnter: string | null; escape: string };

/** Status and its fix, Copy, then More. More's place never depends on the action:
 * shorten the status, then truncate the label, then fold labels to keys by width. */
export function PickerFooter({ library, groupName, hints, altHeld, actions, more, status, feedback, passive, emptyAction }: {
  library: LibraryStatus | null | undefined; groupName: (id: number) => string; hints: KeyHints; altHeld: boolean;
  actions: { copy: () => void; add: () => void; escape: () => void };
  more: React.ReactNode; status?: React.ReactNode; feedback?: string | null; passive?: boolean; emptyAction?: boolean;
}) {
  const bar = useRef<HTMLElement>(null);
  const hintsRow = useRef<HTMLElement>(null);
  const menu = useRef<HTMLSpanElement>(null);
  const statusSize = useRef<HTMLSpanElement>(null);
  const statusMinimum = useRef<HTMLSpanElement>(null);
  const labelMinimum = useRef<HTMLSpanElement>(null);
  const [fit, setFit] = useState({ status: false, labels: false });
  const platform = currentPlatform();
  // biome-ignore lint/correctness/useExhaustiveDependencies: labels, Alt and platform change intrinsic control widths.
  useLayoutEffect(() => {
    const element = bar.current;
    if (!element) return;
    const measure = () => {
      const width = (node: Element | null) => node?.getBoundingClientRect().width ?? 0;
      const available = element.clientWidth;
      const preferred = width(statusSize.current);
      if (!(available > 0 && preferred > 0)) return;
      const gap = parseFloat(getComputedStyle(element).columnGap) || 0;
      const fix = width(element.querySelector(".source-action"));
      const intrinsic = (button: Element | null) => {
        const label = button?.querySelector<HTMLElement>(".key-hint-label");
        if (!label) return width(button);
        const labelGap = label.parentElement ? parseFloat(getComputedStyle(label.parentElement).columnGap) || 0 : 0;
        // A truncated button is narrower than its name; fit by what it wants.
        return width(button) + (getComputedStyle(label).position === "absolute" ? label.scrollWidth + labelGap : Math.max(0, label.scrollWidth - label.clientWidth));
      };
      const actions = [...(hintsRow.current?.children ?? [])];
      const hintGap = hintsRow.current ? parseFloat(getComputedStyle(hintsRow.current).columnGap) || 0 : 0;
      const moreWidth = intrinsic(menu.current?.querySelector("button") ?? null);
      const status = preferred + fix + moreWidth + actions.reduce((sum, action) => sum + intrinsic(action), 0)
        + Math.max(0, actions.length - 1) * hintGap + gap * 2 > available;
      // Never the action's own width: its label must not be able to resize More.
      const labels = width(statusMinimum.current) + fix + moreWidth + width(labelMinimum.current) + gap * 2 > available;
      setFit((old) => old.status === status && old.labels === labels ? old : { status, labels });
    };
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    for (const node of [bar.current, statusSize.current, hintsRow.current, menu.current, bar.current?.querySelector(".source-action")]) if (node) observer?.observe(node);
    window.addEventListener("resize", measure);
    return () => { observer?.disconnect(); window.removeEventListener("resize", measure); };
  }, [hints.enter, hints.shiftEnter, hints.escape, altHeld, platform, status]);
  return <footer data-menu-floor>
    {/* The region includes More and Copy. Its name must stay distinct from
        the Library button, so native name lookup cannot activate the region. */}
    <section ref={bar} aria-label={pickerText.pickerActions} className="library-bar" data-empty-action={emptyAction || undefined}
      data-status-compact={fit.status || undefined} data-hints-compact={fit.labels || undefined}>
      <span className="footer-fit-measure" aria-hidden="true" inert><span ref={statusSize} className="footer-status-size" /><span ref={statusMinimum} className="footer-status-minimum" /><span ref={labelMinimum} className="footer-label-minimum" /></span>
      {status ?? <LibraryStatusLine library={library} groupName={groupName} />}
      <small key="hints" ref={hintsRow} className="enter-hints" aria-label={keyText.footer} role="group">
        {altHeld ? <>
          {hints.shiftEnter && <FooterAction label={hints.shiftEnter} keys={keysFor("add")} onAction={actions.add} />}
          <FooterAction label={hints.escape} keys={keysFor("escape")} onAction={actions.escape} />
        </> : hints.enter ? <FooterAction label={hints.enter} keys={keysFor("copy")} primary onAction={actions.copy} />
          : emptyAction && <FooterAction label={keyText.copy} keys={keysFor("copy")} disabled onAction={actions.copy} />}
      </small>
      <span key="more" ref={menu} className="more">{more}</span>
      <QuietStatus passive={!library?.note && passive}>{library?.note ?? feedback}</QuietStatus>
    </section>
  </footer>;
}

function FooterAction({ label, keys, primary, onAction, disabled }: { label: string; keys: string; primary?: boolean; onAction: () => void; disabled?: boolean }) {
  return <Button variant={primary ? "primary" : "quiet"} size="small" className="footer-action" label={label} keys={keys} isDisabled={disabled} onPress={onAction} />;
}
