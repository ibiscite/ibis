import type { ComponentPropsWithRef, ReactNode } from "react";
import { ListBox, ListBoxItem } from "react-aria-components";
import "./SideNavigation.css";

function SideRow({ page, tile, children }: { page: string; tile?: ReactNode; children: ReactNode }) {
  return <>
    <span className="ui-side-navigation-tile" data-page={page} aria-hidden="true">{tile}</span>
    {children}
  </>;
}

/** A single current page, with React Aria's roving focus, arrows and Home/End. */
export function SideNavigation<T extends string>({ label, value, options, onChange }: {
  label: string;
  value: T;
  options: readonly { value: T; label: string; tile?: ReactNode; gapBefore?: boolean }[];
  onChange: (value: T) => void;
}) {
  return (
    <ListBox className="ui-side-navigation" aria-label={label} selectionMode="single" selectionBehavior="replace"
      disallowEmptySelection selectedKeys={[value]} onSelectionChange={(keys) => {
        const next = keys === "all" ? undefined : keys.values().next().value;
        if (next !== undefined) onChange(next as T);
      }} onAction={(key) => onChange(key as T)}>
      {options.map((option) => (
        <ListBoxItem key={option.value} id={option.value} textValue={option.label} data-gap-before={option.gapBefore || undefined} className="ui-side-navigation-item"
          onPressStart={(event) => {
            // React Aria suppresses unchanged selection; mouse selection happens on press start.
            if (event.pointerType === "mouse" && option.value === value) onChange(option.value);
          }}
          render={(native) => <div {...native as ComponentPropsWithRef<"div">} aria-current={option.value === value ? "page" : undefined} />}>
          <SideRow page={option.value} tile={option.tile}>{option.label}</SideRow>
        </ListBoxItem>
      ))}
    </ListBox>
  );
}

/** Search results are sidebar rows: the page's tile, then the setting, read as one list. */
export function SideResults({ label, status, empty, results, onChoose }: {
  label: string;
  status: string;
  empty: string;
  results: readonly { id: string; label: string; page: string; pageLabel: string; tile?: ReactNode }[];
  onChoose: (id: string) => void;
}) {
  if (!results.length) return <p className="ui-side-navigation-empty" role="status">{empty}</p>;
  return <>
    <p className="visually-hidden" role="status">{status}</p>
    <ListBox className="ui-side-navigation" aria-label={label} selectionMode="none" onAction={(key) => onChoose(String(key))}>
      {results.map((result) => <ListBoxItem key={result.id} id={result.id} textValue={result.label} aria-label={`${result.label}, ${result.pageLabel}`} className="ui-side-navigation-item">
        <SideRow page={result.page} tile={result.tile}>{result.label}</SideRow>
      </ListBoxItem>)}
    </ListBox>
  </>;
}
