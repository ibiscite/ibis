import type { ComponentProps, ReactNode } from "react";
import { Icon } from "../icons";
import { Button } from "./Button";
import "./CitationToken.css";

type TokenButton = Omit<ComponentProps<typeof Button>, "variant" | "size" | "className" | "label" | "keys" | "icon" | "children"> & { children: ReactNode };

/** One reference in the citation line (decision 0042): its own words from the
 * output in a quiet capsule, with a quiet handle when those words do not name
 * it, and the chevrons every pop-up has, saying it opens its details. The line
 * owns its keys; a token is only a button. */
export function CitationToken({ reference, referenceKey, built = false, handle, children, ...button }: TokenButton & {
  reference: string; referenceKey: string;
  /** In the citation being built, rather than the highlighted result shown alone. */
  built?: boolean;
  /** First author and year, for words that do not name the reference ("1" in IEEE). */
  handle?: string;
}) {
  return <span className="citation-token" data-reference={reference} data-reference-key={referenceKey} data-built={built || undefined}>
    <Button {...button} variant="quiet" size="small" className="citation-token-button">
      <span className="citation-token-words">{children}</span>
      {handle && <span className="citation-token-handle"><bdi dir="auto">{handle}</bdi></span>}
      <Icon name="chevrons" className="citation-token-opens" />
    </Button>
  </span>;
}
