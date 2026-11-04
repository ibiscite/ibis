import { useEffect, useMemo, useRef, type ComponentPropsWithRef } from "react";
import { ListBox, ListBoxItem, type Selection } from "react-aria-components";
import "./PageStrip.css";

/** Page thumbnails with their printed numbers, as Preview's sidebar: a click
 * chooses a page, ⇧-click a range, ⌘-click adds or removes one; ↑ ↓ and Space
 * do the same by key. React Aria owns that selection; the strip owns only how a
 * page looks. Thumbnails draw when they come into view and clear when they leave. */
export function PageStrip({ label, labels, selected, current, unlabelled, onChange, draw }: {
  label: string;
  labels: readonly (string | null)[];
  selected: ReadonlySet<number>;
  /** The page in view: a quiet ring, never the selection's fill. */
  current: number;
  /** What a page without a printed number shows instead ("PDF 8"). */
  unlabelled: (index: number) => string;
  onChange: (selected: Set<number>, changed: number | null) => void;
  draw: (index: number, into: HTMLElement) => () => void;
}) {
  // One observer for the strip; each thumbnail joins it when React Aria mounts
  // it, draws while near the view and is cleared when it leaves or unmounts.
  const thumbs = useMemo(() => {
    const drawn = new Map<Element, () => void>();
    const observer = typeof IntersectionObserver === "undefined" ? null : new IntersectionObserver((entries) => {
      for (const entry of entries) {
        const node = entry.target as HTMLElement;
        if (entry.isIntersecting && !drawn.has(node)) drawn.set(node, draw(Number(node.dataset.page), node));
        else if (!entry.isIntersecting) { drawn.get(node)?.(); drawn.delete(node); }
      }
    }, { rootMargin: "50% 0px" });
    return {
      join: (node: HTMLElement | null) => { if (node) observer?.observe(node); },
      stop: () => { observer?.disconnect(); for (const stop of drawn.values()) stop(); drawn.clear(); },
    };
  }, [draw]);
  useEffect(() => thumbs.stop, [thumbs]);
  // React Aria's own selection carries the ⇧-click anchor; hand it back while it
  // still says what the Page value says, so ranges extend from the last click.
  const last = useRef<Selection | null>(null);
  const same = last.current !== null && last.current !== "all" && last.current.size === selected.size && [...selected].every((index) => (last.current as Set<string>).has(String(index)));
  const keys = same ? last.current! : new Set([...selected].map(String));
  return <ListBox aria-label={label} className="page-strip scroll-area" selectionMode="multiple" selectionBehavior="replace"
    selectedKeys={keys} onSelectionChange={(next: Selection) => {
      last.current = next;
      const chosen = next === "all" ? new Set(labels.map((_, i) => i)) : new Set([...next].map(Number));
      const added = [...chosen].find((index) => !selected.has(index));
      onChange(chosen, added ?? null);
    }}>
    {labels.map((printed, index) => <ListBoxItem key={index} id={String(index)} textValue={printed ?? unlabelled(index)} className="page-strip-page"
      render={(props) => <div {...props as ComponentPropsWithRef<"div">} data-current={index === current || undefined} data-unlabelled={printed === null || undefined} />}>
      <span ref={thumbs.join} className="page-strip-paper" data-page={index} aria-hidden="true" />
      <span className="page-strip-label">{printed ?? unlabelled(index)}</span>
    </ListBoxItem>)}
  </ListBox>;
}
