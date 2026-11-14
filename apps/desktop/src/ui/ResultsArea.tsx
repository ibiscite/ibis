import { createContext, useContext, type ComponentPropsWithRef, type ReactNode } from "react";
import { usePresence, usePresenceSlot } from "./presence";

type Layout = "choices" | "empty" | "opening" | "options";
const Layouts = createContext<((slot: Layout, on: boolean) => void) | undefined>(undefined);

/** The results viewport owns scrolling and centring, including delayed empty content. */
export function ResultsArea({ options, children, ...props }: ComponentPropsWithRef<"div"> & { options: boolean }) {
  const { counts, register } = usePresence({ choices: 0, empty: 0, opening: 0, options: 0 });
  return <Layouts.Provider value={register}>
    <div {...props} className="results-area scroll-area" data-options={options || counts.options ? "" : undefined}
      data-centered={counts.choices || counts.empty || counts.opening ? "" : undefined} data-opening={counts.opening ? "" : undefined}>{children}</div>
  </Layouts.Provider>;
}

export function useResultsLayout(layout: Layout, present = true) {
  usePresenceSlot(useContext(Layouts), layout, present);
}

export function ResultsEmpty({ children }: { children: ReactNode }) {
  useResultsLayout("empty");
  return <div className="empty-state empty-template">{children}</div>;
}
