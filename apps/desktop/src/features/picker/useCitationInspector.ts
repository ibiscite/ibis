import { useCallback, useState } from "react";
import type { ShelfCopied } from "./session";

/** What the inspector shows. Bound to the presentation and copy receipt it was
 * opened under, so a later window or copy retires it. */
export type InspectorPanel = {
  kind: "reference" | "failure" | "warnings";
  /** The control that opened it, for Esc when the reference has no segment. */
  origin: HTMLElement;
  presentation: number | undefined;
  reference?: string;
  receipt?: ShelfCopied | null;
};

/** The inspector's one owner, above the shelf because Esc, the list's notices
 * and the footer's Esc hint all answer to it. */
export function useCitationInspector() {
  const [panel, show] = useState<InspectorPanel | null>(null);
  const close = useCallback(() => show(null), []);
  return { panel, show, close };
}

export type CitationInspectorState = ReturnType<typeof useCitationInspector>;
