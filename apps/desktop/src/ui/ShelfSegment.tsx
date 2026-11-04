import type { ComponentProps } from "react";
import { Button } from "./Button";
import { IconButton } from "./IconButton";
import "./ShelfSegment.css";

type SegmentButton = Omit<ComponentProps<typeof Button>, "variant" | "size" | "className" | "label" | "keys" | "icon">;

/** One reference in the shelf's citation [C7]: a quiet button that opens its
 * details and, for a reference that was added, a small × that removes it. */
export function ShelfSegment({ reference, referenceKey, remove, children, ...button }: SegmentButton & {
  reference: string; referenceKey: string;
  /** Present only for a reference in the citation: its accessible name and action. */
  remove?: { label: string; onPress: () => void };
}) {
  return <span className="shelf-segment" data-reference={reference} data-reference-key={referenceKey} data-built={remove ? true : undefined}>
    <Button {...button} variant="quiet" size="small" className="shelf-segment-button">{children}</Button>
    {remove && <IconButton className="shelf-remove" excludeFromTabOrder aria-label={remove.label} onPress={remove.onPress} />}
  </span>;
}
