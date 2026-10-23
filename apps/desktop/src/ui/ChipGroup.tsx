import { useCallback, useEffect, useRef, type ComponentPropsWithRef, type KeyboardEvent, type RefObject } from "react";
import { composing as isComposing } from "../composition";
import { I18nProvider, Tag, TagGroup, TagList, useLocale } from "react-aria-components";
import { Button } from "./Button";
import "./ChipGroup.css";

export type ChipItem = { id: string; label: string; textValue: string; removeLabel: string };
type Props = {
  label: string; items: readonly ChipItem[]; onRemove: (keys: ReadonlySet<string>) => void;
  fallbackFocusRef: RefObject<HTMLElement | null>; locale?: string;
};

/** Controlled labels: the caller owns identity, membership and order. */
export function ChipGroup(props: Props) {
  return props.locale ? <I18nProvider locale={props.locale}><Chips {...props} /></I18nProvider> : <Chips {...props} />;
}

function Chips({ label, items, onRemove, fallbackFocusRef }: Props) {
  const { direction } = useLocale();
  const group = useRef<HTMLDivElement | null>(null);
  const fallback = useRef(fallbackFocusRef);
  fallback.current = fallbackFocusRef;
  const emptyRemoval = useRef(false);
  const composing = useRef(false);
  const restore = useCallback(() => {
    const active = document.activeElement;
    if ((active === document.body || group.current?.contains(active)) && fallback.current.current?.isConnected) {
      fallback.current.current.focus({ preventScroll: true });
    }
    emptyRemoval.current = false;
  }, []);
  const ref = useCallback((node: HTMLDivElement | null) => {
    // A caller may remove the empty group in the same update as its last chip.
    if (!node && emptyRemoval.current) restore();
    group.current = node;
  }, [restore]);
  useEffect(() => {
    // React Aria focuses a retained empty group first; then use the caller's
    // destination, without taking focus from an unrelated control.
    if (items.length === 0 && emptyRemoval.current) restore();
  }, [items.length, restore]);
  const guard = (event: KeyboardEvent) => {
    if (isComposing(event, composing.current)) event.stopPropagation();
  };
  return <TagGroup ref={ref} className="chip-group" aria-label={label} dir={direction} selectionMode="none"
    render={(props) => <div {...props as ComponentPropsWithRef<"div">}
      onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }}
      onKeyDownCapture={guard} onKeyUpCapture={guard} />}
    onRemove={(keys) => {
      emptyRemoval.current = items.length > 0 && items.every((item) => keys.has(item.id)) && Boolean(group.current?.contains(document.activeElement));
      onRemove(new Set([...keys].map(String)));
    }}>
    <TagList items={items} className="chip-group-list">
      {(item) => <Tag id={item.id} textValue={item.textValue} className="chip-group-item">
        <bdi>{item.label}</bdi>
        <Button slot="remove" variant="icon" size="small" className="chip-group-remove"
          render={(props) => <button {...props as ComponentPropsWithRef<"button">} aria-label={item.removeLabel} aria-labelledby={undefined} />}>
          <span aria-hidden="true">×</span>
        </Button>
      </Tag>}
    </TagList>
  </TagGroup>;
}
