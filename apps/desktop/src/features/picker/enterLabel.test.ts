import { expect, test } from "vitest";
import { enterAction, enterOutcome, type EnterInputs, type EnterOutcome } from "./enterLabel";

const base: EnterInputs = {
  count: 1, preview: "ready", editor: false, findOpen: false, practising: false, deciding: false, changing: false,
  format: "pandoc", app: "Invented Editor", canPaste: true, stayOpen: false, ownOrder: false, refusal: null,
};

// Every place a citation can go.
const destinations: Record<string, Partial<EnterInputs>> = {
  "an app that pastes": {},
  "an app that only copies": { canPaste: false },
  "no app": { app: null, canPaste: false },
  "an app, window kept open": { stayOpen: true },
};
// Every outcome Enter can have, by the state that gives it.
const outcomes: Record<string, Partial<EnterInputs>> = {
  "nothing offered": { count: 0 },
  "out of date": { preview: "stale" },
  failed: { preview: "failed" },
  editor: { editor: true },
  "find mode": { findOpen: true },
  practice: { practising: true },
  "the practice offer still being read": { deciding: true },
  "changing the last citation": { changing: true },
  delivery: {},
  refused: { refusal: "A key in thesis.bib identifies a different work." },
};
// What each outcome's words must say: the bar's action is what Enter does.
const words: Record<EnterOutcome, RegExp | null> = {
  nothing: null, send: /^Send to Editor$/, open: /^Open PDF/, practice: /^Paste Here to Try$/, deciding: null,
  change: /^Change Citation$/, paste: /^Paste into Invented Editor$/, copy: /^Copy$/,
};

test("the bar names every Enter outcome, for every destination, as Enter does it", () => {
  for (const [place, destination] of Object.entries(destinations)) for (const [state, outcome] of Object.entries(outcomes)) {
    const inputs = { ...base, ...destination, ...outcome };
    const action = enterAction(inputs);
    expect(action.outcome, `${state} in ${place}`).toBe(enterOutcome(inputs));
    const expected = words[action.outcome];
    if (expected) expect(action.label, `${state} in ${place}`).toMatch(expected);
    else if (action.outcome === "nothing") expect(action.label, `${state} in ${place}`).toBeNull();
    // Undecided, the action keeps the delivery's room but is hidden: it names nothing Enter may not do.
    expect(action.hidden, `${state} in ${place}`).toBe(action.outcome === "deciding");
    // Paste only where Enter pastes: an app that pastes, the window not kept open.
    const pastes = place === "an app that pastes";
    if (["delivery", "refused"].includes(state)) expect(action.outcome, `${state} in ${place}`).toBe(pastes ? "paste" : "copy");
    // The format is the header's: never in the action.
    expect(action.label ?? "", `${state} in ${place}`).not.toMatch(/Pandoc/);
    // No mode word beside a verb that already says it.
    expect(action.label ?? "", `${state} in ${place}`).not.toMatch(/Automatic paste|Clipboard/);
    // A refusal keeps the words and turns the action off.
    expect(action.off, `${state} in ${place}`).toBe(state === "refused");
  }
});

test("each outcome is reached by its own state, in Enter's order", () => {
  expect(enterOutcome({ ...base, editor: true, findOpen: true, practising: true })).toBe("send");
  expect(enterOutcome({ ...base, findOpen: true, practising: true })).toBe("open");
  expect(enterOutcome({ ...base, practising: true, changing: true })).toBe("practice");
  expect(enterOutcome({ ...base, deciding: true, changing: true })).toBe("deciding");
  expect(enterOutcome({ ...base, deciding: true, findOpen: true })).toBe("open");
  expect(enterOutcome({ ...base, changing: true })).toBe("change");
  expect(enterOutcome({ ...base, count: 0, editor: true })).toBe("nothing");
});

test("counts, entries and the person's order are named; the destination once", () => {
  expect(enterAction({ ...base, count: 2 }).label).toBe("Paste into Invented Editor");
  expect(enterAction({ ...base, count: 2, canPaste: false }).label).toBe("Copy 2 Citations");
  expect(enterAction({ ...base, format: "ieee" }).label).toBe("Paste Entry into Invented Editor");
  expect(enterAction({ ...base, format: "ieee", count: 2 }).label).toBe("Paste 2 Entries into Invented Editor");
  expect(enterAction({ ...base, format: "ieee", count: 2, app: null }).label).toBe("Copy 2 Entries");
  expect(enterAction({ ...base, format: "apa", count: 2, ownOrder: true }).label).toBe("Paste into Invented Editor · your order");
});
