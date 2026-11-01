import type { ComponentPropsWithRef, ReactNode } from "react";
import { ListBox, ListBoxItem } from "react-aria-components";
import "./SideNavigation.css";

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
          <span className="ui-side-navigation-tile" data-page={option.value} aria-hidden="true">{option.tile}</span>
          {option.label}
        </ListBoxItem>
      ))}
    </ListBox>
  );
}
