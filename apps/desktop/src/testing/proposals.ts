import type { WaitingProposal } from "../bindings";

/** Invented proposals shared by the Settings demo and IPC fake. */
export function exampleProposals(): WaitingProposal[] {
  return [
    { id: "0123456789abcdef", source_app: "Claude Desktop", library: "signals.bib", summary: "Change title of “River signals”.", created_ms: 1794744000000,
      lines: ["title: River signals revisited"], before: "@article{vale2026river,\n  title = {River signals},\n}", after: "@article{vale2026river,\n  title = {River signals revisited},\n}", refusal: null },
    { id: "fedcba9876543210", source_app: "Cursor", library: "signals.bib", summary: "Add “A map of the reeds” as lin2026map.", created_ms: 1794740400000,
      lines: ["title: A map of the reeds"], before: null, after: "@book{lin2026map,\n  title = {A map of the reeds},\n  author = {Lin, Ada},\n  year = {2026},\n}", refusal: null },
  ];
}
