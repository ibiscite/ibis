import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { Input, Label, TextField } from "react-aria-components";
import { isolate, say } from "../../i18n";
import { Button } from "../../ui/Button";
import { PageStrip } from "../../ui/PageStrip";
import { confirmOffset, formatPages, parsePages } from "./pages";
import type { PageSource } from "./source";
import "../../ui/Reader.css";
import "../../ui/Overlay.css";

/** A reference's pages beside its details (decision 0038): thumbnails, the
 * pages, and one toolbar line. The Page value is the one truth: the thumbnails
 * show it, and choosing a page writes it. Scrolling never changes it. */
export function Reader({ source, title, page, onPageChange, onClose }: {
  source: PageSource; title: string; page: string;
  onPageChange: (value: string) => void; onClose: () => void;
}) {
  // Printed numbers the person confirmed this opening, for pages without one.
  // The parent keys a Reader by its document, so a new document starts afresh.
  const [labels, setLabels] = useState(source.labels);
  const parsed = useMemo(() => parsePages(page, labels).selected, [page, labels]);
  // A chosen page with no printed number waits here until its number is confirmed.
  const [unnumbered, setUnnumbered] = useState<number | null>(null);
  const selected = useMemo(() => unnumbered === null ? parsed : new Set([...parsed, unnumbered]), [parsed, unnumbered]);
  const [current, setCurrent] = useState(() => Math.min(...parsed, labels.length - 1));
  const [anchor, setAnchor] = useState<number | null>(null);
  const [printed, setPrinted] = useState("");
  const pages = useRef<HTMLDivElement>(null);

  const show = (index: number) => pages.current?.querySelector(`[data-page="${index}"]`)?.scrollIntoView?.({ block: "start" });
  const choose = (next: Set<number>, changed: number | null) => {
    const missing = [...next].find((index) => labels[index] === null) ?? null;
    setUnnumbered(missing);
    if (changed !== null) { setAnchor(changed); show(changed); }
    onPageChange(formatPages(labels, next));
  };
  const cite = (extend: boolean) => {
    if (extend && anchor !== null) {
      const next = new Set(selected);
      for (let i = Math.min(anchor, current); i <= Math.max(anchor, current); i++) next.add(i);
      choose(next, null);
    } else { setAnchor(current); choose(new Set([current]), null); }
  };

  // The page in view is the one whose top is nearest the pages' top.
  // biome-ignore lint/correctness/useExhaustiveDependencies: one document per Reader (keyed by its parent).
  useEffect(() => {
    const node = pages.current;
    if (!node || typeof IntersectionObserver === "undefined") return;
    const drawn = new Map<Element, () => void>();
    const visible = new Set<number>();
    const observer = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        const index = Number((entry.target as HTMLElement).dataset.page);
        if (entry.isIntersecting) { visible.add(index); if (!drawn.has(entry.target)) drawn.set(entry.target, source.draw(index, entry.target.firstElementChild as HTMLElement)); }
        else { visible.delete(index); drawn.get(entry.target)?.(); drawn.delete(entry.target); }
      }
      if (visible.size) setCurrent(Math.min(...visible));
    }, { root: node, threshold: [0, 0.5] });
    for (const sheet of node.querySelectorAll(".reader-sheet")) observer.observe(sheet);
    return () => { observer.disconnect(); for (const stop of drawn.values()) stop(); };
  }, []);
  // biome-ignore lint/correctness/useExhaustiveDependencies: opens at the first cited page, once.
  useEffect(() => { if (parsed.size) show(Math.min(...parsed)); }, []);

  const keys = (event: KeyboardEvent) => {
    const command = event.metaKey || event.ctrlKey;
    if (event.key === "Enter" && command) { event.preventDefault(); cite(event.shiftKey); }
    else if (event.altKey && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
      event.preventDefault(); show(Math.max(0, Math.min(labels.length - 1, current + (event.key === "ArrowDown" ? 1 : -1))));
    }
    else if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); onClose(); }
  };
  const label = labels[current];
  const ask = unnumbered !== null && <form className="reader-ask" onSubmit={(event) => {
    event.preventDefault();
    const next = confirmOffset(labels, unnumbered, printed);
    if (next[unnumbered] === null) return;
    setLabels(next); setUnnumbered(null); setPrinted("");
    onPageChange(formatPages(next, new Set([...parsed, unnumbered])));
  }}>
    <TextField className="reader-ask-field" value={printed} onChange={setPrinted} autoFocus>
      <Label>{say("reader-ask", { index: String(unnumbered + 1) })}</Label>
      <Input inputMode="numeric" />
    </TextField>
    <Button size="small" type="submit">{say("reader-use-nearby")}</Button>
  </form>;
  // As Preview: thumbnails in a sidebar, the pages on one canvas edge to edge, and
  // the page you are on in a small floating bar over them, with the one action
  // that acts on it. No toolbar row takes height from the page.
  return <section className="reader" aria-label={say("reader-region", { title })} onKeyDown={keys}>
    <div className="reader-strip"><PageStrip label={say("reader-thumbnails")} labels={labels} selected={selected} current={current} unlabelled={(index: number) => say("reader-pdf-page", { index: String(index + 1) })} onChange={choose} draw={source.draw} /></div>
    <div className="reader-view">
      <div ref={pages} className="reader-pages scroll-area" tabIndex={0} aria-label={say("reader-pages")}>
        {labels.map((printed, index) => <div key={index} className="reader-sheet" data-page={index} role="group" aria-label={printed ? say("reader-indicator", { printed: isolate(printed), index: String(index + 1), count: String(labels.length) }) : say("reader-unknown", { index: String(index + 1), count: String(labels.length) })}>
          <div className="reader-paper" />
        </div>)}
      </div>
      <div className="reader-hud ui-overlay" role="group" aria-label={say("reader-page-in-view")}>
        {ask || <>
          <span className="reader-indicator" aria-live="polite">{label ? say("reader-indicator", { printed: isolate(label), index: String(current + 1), count: String(labels.length) }) : say("reader-unknown", { index: String(current + 1), count: String(labels.length) })}</span>
          <Button size="small" onPress={() => cite(false)}>{say("reader-cite")}</Button>
        </>}
      </div>
    </div>
  </section>;
}
