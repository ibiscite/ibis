import { useId, type ComponentPropsWithRef, type ReactNode } from "react";
import { ListBox, ListBoxItem } from "react-aria-components";
import { ariaKeys, keyLabel } from "../keys";
import { currentPlatform } from "../platform";
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

/** The choice screen pattern (docs/design.md, First run and choice screens
 * [C12]): a title and one line on the tile column's edge, then result-style
 * rows. Arrow keys move, Enter or a click chooses, and focus stays on the row. */
export function ChoiceList({ title, line, choices, onChoose, footnote, autoFocus = false }: {
  title: ReactNode;
  line?: ReactNode;
  choices: readonly Choice[];
  onChoose: (id: string) => void;
  /** Quiet text below the rows. */
  footnote?: ReactNode;
  autoFocus?: boolean;
}) {
  const heading = useId();
  const mac = currentPlatform() === "macos";
  return (
    <section className="choice-screen" aria-labelledby={heading}>
      <h2 className="choice-title" id={heading} dir="auto">{title}</h2>
      {line && <p className="choice-line">{line}</p>}
      <ListBox className="choice-list" aria-labelledby={heading} selectionMode="none" autoFocus={autoFocus ? "first" : undefined}
        onAction={(id) => onChoose(String(id))}>
        {choices.map((choice) => (
          <ListBoxItem key={choice.id} id={choice.id} textValue={choice.title} className="choice-row"
            render={(props) => <div {...props as ComponentPropsWithRef<"div">} aria-keyshortcuts={choice.keys ? ariaKeys(choice.keys, mac) : undefined} />}
            data-suggested={choice.suggested || undefined}>
            <span className="result-tile choice-tile" aria-hidden="true">{choice.icon}</span>
            <span className="result-body">
              <span className="result-title" dir="auto">{choice.title}</span>
              {choice.help && <span className="result-meta choice-help" dir="auto">{choice.help}</span>}
            </span>
            {choice.tag && <span className="result-year choice-tag" dir="auto">{choice.tag}</span>}
            {choice.keys && <span className="result-year choice-tag" aria-hidden="true"><bdi dir="ltr">{keyLabel(choice.keys, mac)}</bdi></span>}
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
