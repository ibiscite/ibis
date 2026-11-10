import type { ComponentProps, ReactNode } from "react";
import { Button } from "./Button";
import { IconButton } from "./IconButton";
import "./CitationToken.css";

type TokenButton = Omit<ComponentProps<typeof Button>, "variant" | "size" | "className" | "label" | "keys" | "icon" | "children"> & { children: ReactNode };

/** One reference in the citation line (decision 0042): its own words from the
 * output on a soft fill, a quiet handle when those words do not name it, and,
 * for a reference in the citation, a small × that is always shown. Neither the
 * fill nor the × is pasted. The line owns the token's keys. */
export function CitationToken({ reference, referenceKey, handle, remove, children, ...button }: TokenButton & {
  reference: string; referenceKey: string;
  /** First author and year, for words that do not name the reference ("1" in IEEE). */
  handle?: string;
  /** Present for a reference in the citation being built: its name and action. */
  remove?: { label: string; onPress: () => void };
}) {
  return <span className="citation-token" data-reference={reference} data-reference-key={referenceKey} data-built={remove ? true : undefined}>
    <Button {...button} variant="quiet" size="small" className="citation-token-button">
      <span className="citation-token-words">{children}</span>
      {handle && <span className="citation-token-handle"><bdi dir="auto">{handle}</bdi></span>}
    </Button>
    {remove && <IconButton className="citation-token-remove" excludeFromTabOrder aria-label={remove.label} onPress={remove.onPress} />}
  </span>;
}
