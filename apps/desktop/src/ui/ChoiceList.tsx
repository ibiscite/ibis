import { useEffect, useId, useRef, type ComponentPropsWithRef, type ReactNode } from "react";
import { ListBox, ListBoxItem } from "react-aria-components";
import { ariaKeys, keyLabel } from "../keys";
import { currentPlatform } from "../platform";
import { useResultsLayout } from "./ResultsArea";
import { Stack } from "./Stack";
import "./ChoiceList.css";

/** One row of a choice screen: a tile, a title, one help line and an optional trailing tag. */
export type Choice = {
  id: string;
  title: string;
  help?: string;
  /** A short fact at the row's end, such as "Found on this Mac". */
  tag?: string;
  keys?: string;
  icon: ReactNode;
  /** The suggested way forward: an accent tile. One per screen at most. */
  suggested?: boolean;
};

/** The choice screen pattern (docs/design/choice-screens.md
 * [C12]): one centred group, with result-style rows or the welcome grid.
 * React Aria owns spatial arrow navigation, activation and focus. */
export function ChoiceList({ title, line, choices, onChoose, footnote, autoFocus = false, grid = false, compact = false, mark, focusId }: {
  title: ReactNode;
  line?: ReactNode;
  choices: readonly Choice[];
  onChoose: (id: string) => void;
  /** Quiet text below the rows. */
  footnote?: ReactNode;
  autoFocus?: boolean;
  grid?: boolean;
  /** Beside a list of results: its rows on the list's columns, its title for
   * screen readers only, and nothing centred, so the results keep the room. */
  compact?: boolean;
  mark?: ReactNode;
  /** Restore the tile that opened a replaced choice screen. */
  focusId?: string;
}) {
  const heading = useId();
  useResultsLayout("choices", !compact);
  useResultsLayout("options", choices.length > 0);
  const mac = currentPlatform() === "macos";
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
    <section className="choice-screen" data-grid={grid || undefined} data-compact={compact || undefined} aria-labelledby={heading}>
      {mark}
      <Stack gap="--space-6"><Stack gap="--space-1">
        <h2 className={compact ? "visually-hidden" : "choice-title"} id={heading} dir="auto">{title}</h2>
        {line && <p className="choice-line">{line}</p>}
      </Stack><Stack gap="--space-4">
      <ListBox ref={list} className="choice-list" layout={grid ? "grid" : "stack"} aria-labelledby={heading} selectionMode="none" autoFocus={autoFocus && !focusId ? "first" : undefined}
        onAction={(id) => onChoose(String(id))}>
        {choices.map((choice) => (
          <ListBoxItem key={choice.id} id={choice.id} textValue={choice.title} className="choice-row"
            render={(props) => <div {...props as ComponentPropsWithRef<"div">} aria-keyshortcuts={choice.keys ? ariaKeys(choice.keys, mac) : undefined} />}
            data-suggested={choice.suggested || undefined}>
            {/* Its own parts, not a result row's: the list's density and resting-row rules never reach a choice. */}
            <span className="choice-tile" aria-hidden="true">{choice.icon}</span>
            <span className="choice-body">
              {/* The key follows its title's words, on their line, so it never floats at the card's far edge. */}
              <span className="choice-line-title"><span className="choice-name" dir="auto">{choice.title}</span>
                {choice.keys && <span className="choice-keys" aria-hidden="true"><bdi dir="ltr">{keyLabel(choice.keys, mac)}</bdi></span>}</span>
              {choice.help && <span className="choice-help" dir="auto">{choice.help}</span>}
            </span>
            {choice.tag && <span className="choice-tag" dir="auto" title={choice.tag}>{choice.tag}</span>}
          </ListBoxItem>
        ))}
      </ListBox>
      {footnote && <p className="choice-line choice-footnote">{footnote}</p>}
      </Stack></Stack>
    </section>
  );
}

/** Moves focus to the first row of the choice screen shown, if any. */
export function focusChoices(root: ParentNode = document): boolean {
  const row = root.querySelector<HTMLElement>(".choice-list [role='option']");
  row?.focus();
  return row !== null;
}
