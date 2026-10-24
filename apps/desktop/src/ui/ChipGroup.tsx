import { useCallback, useContext, useEffect, useLayoutEffect, useRef, type ComponentPropsWithRef, type KeyboardEvent, type ReactNode, type RefObject } from "react";
import { composing as isComposing } from "../composition";
import { ListKeyboardDelegate } from "react-aria";
import { I18nProvider, ListStateContext, Tag, TagGroup, TagList, useLocale } from "react-aria-components";
import { Button } from "./Button";
import "./ChipGroup.css";

export type ChipItem = { id: string; label: string; textValue: string; removeLabel: string; content?: ReactNode; before?: ReactNode; after?: ReactNode };
type Props = {
  label: string; items: readonly ChipItem[]; onRemove: (keys: ReadonlySet<string>) => void;
  fallbackFocusRef: RefObject<HTMLElement | null>; locale?: string;
  variant?: "chips" | "citation"; dir?: "ltr" | "rtl" | "auto";
};

/** Controlled labels: the caller owns identity, membership and order. */
export function ChipGroup(props: Props) {
  return props.locale ? <I18nProvider locale={props.locale}><Chips {...props} /></I18nProvider> : <Chips {...props} />;
}

function Chips({ label, items, onRemove, fallbackFocusRef, variant = "chips", dir }: Props) {
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
    else if (variant === "citation" && event.key === "Escape" && !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
      event.preventDefault(); event.stopPropagation();
      fallback.current.current?.focus({ preventScroll: true });
    }
  };
  const remove = (item: ChipItem) => <Button slot="remove" variant="icon" size="small" className="chip-group-remove"
    render={(props) => <button {...props as ComponentPropsWithRef<"button">} aria-label={item.removeLabel} aria-labelledby={undefined} />}>
    <span aria-hidden="true">×</span>
  </Button>;
  const list = <TagList items={items} className="chip-group-list">
    {(item) => <Tag id={item.id} textValue={item.textValue} className="chip-group-item">
      {variant === "citation" ? <>{item.before}<span className="citation-pill">
        <bdi className="citation-text" dir={dir === "ltr" ? "ltr" : "auto"}>{item.content ?? item.label}</bdi>{remove(item)}
      </span>{item.after}</> : <><bdi>{item.label}</bdi>{remove(item)}</>}
    </Tag>}
  </TagList>;
  return <TagGroup ref={ref} className="chip-group" data-variant={variant === "citation" ? variant : undefined}
    aria-label={label} dir={variant === "citation" ? dir ?? direction : direction} selectionMode="none"
    render={(props) => <div {...props as ComponentPropsWithRef<"div">}
      onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }}
      onKeyDownCapture={guard} onKeyUpCapture={guard} />}
    onRemove={(keys) => {
      emptyRemoval.current = items.length > 0 && items.every((item) => keys.has(item.id)) && Boolean(group.current?.contains(document.activeElement));
      onRemove(new Set([...keys].map(String)));
    }}>
    {variant === "citation" ? <CitationKeyboard dir={dir}>{list}</CitationKeyboard> : list}
  </TagGroup>;
}

/** Keep React Aria's collection and focus owner, adjusting only mismatched rendered direction. */
function CitationKeyboard({ dir, children }: { dir?: Props["dir"]; children: ReactNode }) {
  const state = useContext(ListStateContext);
  const { direction: localeDirection } = useLocale();
  const boundary = useRef<HTMLDivElement>(null);
  const renderedDirection = () => {
    const value = boundary.current && getComputedStyle(boundary.current).direction;
    const direction = value || (dir === "rtl" ? "rtl" : dir === "ltr" ? "ltr" : localeDirection);
    if (boundary.current) boundary.current.dataset.direction = direction;
    return direction;
  };
  useLayoutEffect(() => { renderedDirection(); }, [children, dir, localeDirection]);
  const navigate = (event: KeyboardEvent) => {
    if (!state || !boundary.current || !["ArrowLeft", "ArrowRight"].includes(event.key)
      || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey
      || !(event.target instanceof HTMLElement) || event.target.getAttribute("role") !== "row") return;
    const direction = renderedDirection();
    if (direction === localeDirection) return;
    const delegate = new ListKeyboardDelegate({ collection: state.collection, ref: boundary, orientation: "horizontal",
      direction: direction === "rtl" ? "rtl" : "ltr", disabledKeys: state.disabledKeys, disabledBehavior: state.selectionManager.disabledBehavior });
    const key = state.selectionManager.focusedKey;
    if (key === null) return;
    const forward = event.key === "ArrowRight" ? direction === "ltr" : direction === "rtl";
    const next = (event.key === "ArrowRight" ? delegate.getKeyRightOf?.(key) : delegate.getKeyLeftOf?.(key))
      ?? (forward ? delegate.getFirstKey() : delegate.getLastKey());
    if (next !== null) { event.preventDefault(); event.stopPropagation(); state.selectionManager.setFocusedKey(next); }
  };
  return <div ref={boundary} className="citation-keyboard" onFocusCapture={renderedDirection} onPointerEnter={renderedDirection}
    onKeyDownCapture={navigate}>{children}</div>;
}
