import { useEffect, useId, useRef, type ReactNode } from "react";
import { ListBox, ListBoxItem } from "react-aria-components";
import "./ChoiceList.css";
import "./Option.css";

/** One row of a choice screen: a tile, a title, one help line and an optional trailing tag. */
export type Choice = {
  id: string;
  title: string;
  help?: string;
  /** A short fact at the row's end, such as "Found on this Mac". */
  tag?: string;
  icon: ReactNode;
  /** The suggested way forward: an accent tile. One per screen at most. */
  suggested?: boolean;
};

/** The choice screen pattern (docs/design.md, First run and choice screens
 * [C12]): one centred group, with result-style rows or the welcome grid.
 * React Aria owns spatial arrow navigation, activation and focus. */
export function ChoiceList({ title, line, choices, onChoose, footnote, autoFocus = false, grid = false, mark, focusId }: {
  title: ReactNode;
  line?: ReactNode;
  choices: readonly Choice[];
  onChoose: (id: string) => void;
  /** Quiet text below the rows. */
  footnote?: ReactNode;
  autoFocus?: boolean;
  grid?: boolean;
  mark?: ReactNode;
  /** Restore the tile that opened a replaced choice screen. */
  focusId?: string;
}) {
  const heading = useId();
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!focusId) return;
    const frame = requestAnimationFrame(() => {
      const row = [...(list.current?.querySelectorAll<HTMLElement>('[role="option"]') ?? [])]
        .find((node) => node.dataset.key === focusId);
      row?.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [focusId]);
  return (
    <section className="choice-screen" data-grid={grid || undefined} aria-labelledby={heading}>
      {mark}
      <h2 className="choice-title" id={heading} dir="auto">{title}</h2>
      {line && <p className="choice-line">{line}</p>}
      <ListBox ref={list} className="choice-list" layout={grid ? "grid" : "stack"} aria-labelledby={heading} selectionMode="none" autoFocus={autoFocus && !focusId ? "first" : undefined}
        onAction={(id) => onChoose(String(id))}>
        {choices.map((choice) => (
          <ListBoxItem key={choice.id} id={choice.id} textValue={choice.title} className="choice-row"
            data-suggested={choice.suggested || undefined}>
            <span className="result-tile choice-tile" aria-hidden="true">{choice.icon}</span>
            <span className="result-body">
              <span className="result-title" dir="auto">{choice.title}</span>
              {choice.help && <span className="result-meta choice-help" dir="auto">{choice.help}</span>}
            </span>
            {choice.tag && <span className="result-year choice-tag" dir="auto" title={choice.tag}>{choice.tag}</span>}
            <svg className="choice-chevron" aria-hidden="true" focusable="false" width="16" height="16" viewBox="0 0 20 20"
              fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="m8 5 5 5-5 5" />
            </svg>
          </ListBoxItem>
        ))}
      </ListBox>
      {footnote && <p className="choice-line choice-footnote">{footnote}</p>}
    </section>
  );
}

/** Moves focus to the first row of the choice screen shown, if any. */
export function focusChoices(root: ParentNode = document): boolean {
  const row = root.querySelector<HTMLElement>(".choice-list [role='option']");
  row?.focus();
  return row !== null;
}
