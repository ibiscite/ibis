// Drives the guards of canvas/geometry.ts through the canvas scenarios in one
// page, so the runners (gallery/webkit.swift, gallery/canvas.mjs) only choose a
// window size and an appearance and print what this returns. Invented data only.
import { englishMessages, type MessageId } from "../src/i18n";
import { libraryOperations, OPERATION_FAILURES, type Operation } from "../src/features/picker/Notices";
import { HOLD_MS } from "../src/ui/HeldKeys";
import { currentPlatform } from "../src/platform";
import { anchorBoxes, anchorMoves, frameStates, geometry, heldKeys, layerRoles, occlusion, oneSurface, overflow, paintOnly, settle, slots } from "./geometry";

type Mount = (host: HTMLElement, options: { scenario: string; theme: string }) => () => void;

/** Picker screens held to the single-picture layout rules (geometry()). */
export const geometryScreens = ["results", "first-run", "shelf", "reference-details", "show-as-menu", "reader", "commands", "page-in-search", "peek", "update", "undo-notice", "larger-text", "right-to-left", "compact", "long-titles", "stale", "more", "format-menu", "library-menu"];

/** Scenarios with something floating, for the layer check in every appearance. */
export const layerScreens = ["more", "format-menu", "library-menu", "show-as-menu", "reference-details", "peek", "shortcuts", "writing-projects", "cited-since-opening"];

/** Screens checked again while ⌘ (Ctrl elsewhere) is held, as a person holds it:
 * every held key and the row numbers drawn over a list with a citation. */
export const heldScreens = ["shelf", "compact", "right-to-left"];

/** Holds the command key alone for longer than the hold (ui/HeldKeys), returns what
 * the check finds, and lets go. The same keydown a person's key sends. */
async function whileHeld(check: () => string[]): Promise<string[]> {
  const key = currentPlatform() === "macos" ? "Meta" : "Control";
  window.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  await new Promise((done) => setTimeout(done, HOLD_MS + 100));
  await settle();
  try { return check(); } finally {
    window.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true }));
    await settle();
  }
}

/** Each catalogue message a slot can show, with the longest invented values its
 * callers pass. A message added to a slot's caller is added here. */
const tile = "Featherstonehaugh 2026";
export const slotMessages: Record<keyof typeof slots, { id: MessageId; args?: Record<string, string | number>; actions?: true }[]> = {
  noticeSlot: [
    { id: "undo-removed", args: { reference: tile }, actions: true },
    { id: "picker-window-reset", actions: true },
    { id: "picker-undo-failed", actions: true },
    { id: "delight-tip", args: { keys: "⇧⌘P" } },
    { id: "delight-library100" }, { id: "delight-library1000" }, { id: "delight-library10000" },
    { id: "delight-empty" }, { id: "delight-loading" }, { id: "delight-no-library" },
    { id: "delight-zoom", args: { percent: 200 } },
    { id: "picker-shorthand-hint" },
    { id: "update-failed" },
  ],
  noticeLine: [
    // Every failed action the line reports; library failures are the footer's.
    ...(Object.keys(OPERATION_FAILURES) as Operation[]).filter((operation) => !libraryOperations.has(operation)).map((operation) => ({ id: OPERATION_FAILURES[operation] })),
    { id: "picker-search-error" }, { id: "picker-copy-error" },
    { id: "picker-missing-citations", args: { keys: "featherstonehaugh2026, aster2026" } },
    { id: "picker-editor-waiting", args: { format: "Chicago (author-date)" } },
    { id: "picker-shortcut-not-used", args: { reason: "Another app uses it." } },
    { id: "picker-shortcut-unavailable", args: { shortcut: "⌥⇧⌘Y", reason: "Another app uses it." } },
  ],
};

/** Every guard failure in the scenarios this pass covers, as "scenario: failure".
 * frame: every scenario at the default window, plus the anchors across states;
 * narrow: words in their slots and nothing covered, at the narrowest window;
 * short: More in a window so short its list scrolls [C9];
 * layers: the floating surfaces only, in each system and Ibis appearance. */
export async function audit(host: HTMLElement, mount: Mount, scenarios: readonly string[], { pass = "frame", theme = "light" }: { pass?: "frame" | "narrow" | "short" | "layers" | "inventory"; theme?: string } = {}): Promise<string[]> {
  const failures: string[] = [];
  const show = async (scenario: string) => { mount(host, { scenario, theme }); await settle(); };
  const picker = scenarios.filter((scenario) => !["notice", "states", "tokens", "parts"].includes(scenario));

  // Which notice each picker scenario shows, if any: for reviewing the canvas.
  if (pass === "inventory") {
    for (const scenario of picker) {
      await show(scenario);
      const shown = document.querySelector(".ui-notice-line > :is(.ui-line-notice, .notice-slot)");
      const top = (selector: string) => { const node = document.querySelector(selector); return node && node.getBoundingClientRect().height ? Math.round(node.getBoundingClientRect().top) : "-"; };
      const bottom = (selector: string) => { const node = document.querySelector(selector); return node && node.getBoundingClientRect().height ? Math.round(node.getBoundingClientRect().bottom) : "-"; };
      failures.push(`${scenario}: search ends ${bottom(".search-bar")}, line ${top(".ui-notice-line")}–${bottom(".ui-notice-line")}, first row ${top('#results [role="option"]')}; ${shown?.textContent?.trim() || "no notice"}`);
    }
    return failures;
  }

  if (pass === "short") {
    await show("more");
    return geometry().map((failure) => `more (short): ${failure}`);
  }

  if (pass === "layers") {
    for (const scenario of layerScreens.filter((name) => scenarios.includes(name))) {
      await show(scenario);
      failures.push(...layerRoles().map((failure) => `${scenario}: ${failure}`));
      // Again over the Mac's glass, whose translucent layers let the system's appearance through.
      document.documentElement.dataset.nativeAppearance = "liquid-glass";
      failures.push(...layerRoles().map((failure) => `${scenario} (glass): ${failure}`));
    }
    return failures;
  }

  if (pass === "narrow") {
    for (const scenario of picker) {
      await show(scenario);
      failures.push(...occlusion().map((failure) => `${scenario} (narrow): ${failure}`));
    }
    for (const [slot, messages] of Object.entries(slotMessages) as [keyof typeof slots, typeof slotMessages[keyof typeof slots]][]) {
      await show(slots[slot].scenario);
      const words = document.querySelector<HTMLElement>(slots[slot].selector);
      if (!words) { failures.push(`[C24 words] ${slot}: no slot drawn to measure in ${slots[slot].scenario}`); continue; }
      const actions = [...words.parentElement!.children].filter((part): part is HTMLElement => part !== words && part instanceof HTMLElement);
      for (const { id, args, actions: offer } of messages) {
        words.textContent = englishMessages.text(id, args);
        // A remark has the whole line; an offer shares it with its actions.
        for (const action of actions) action.style.display = offer ? "" : "none";
        const problem = overflow(words, slots[slot].lines);
        if (problem) failures.push(`[C24 words] ${slot} "${id}": ${problem} at ${document.documentElement.clientWidth}px`);
      }
    }
    return failures;
  }

  // A message no slot declares is caught wherever a scenario shows it.
  // Each declared message as a pattern: any value may fill its variables.
  const plain = (text: string) => text.replace(/[\u2068\u2069]/g, "");
  const declared = Object.values(slotMessages).flat().map(({ id, args }) => new RegExp(`^${plain(englishMessages.text(id, Object.fromEntries(Object.keys(args ?? {}).map((name) => [name, "\u0001"]))))
    .replace(/[.*+?^${}()|[\]\\]/g, "\\$&").replace(/\u0001/g, ".+")}$`));
  const idle: { boxes?: ReturnType<typeof anchorBoxes> } = {};
  const states = new Map<string, ReturnType<typeof anchorBoxes>>();
  for (const scenario of scenarios.filter((name) => name !== "notice")) {
    await show(scenario);
    const found = [
      ...geometryScreens.includes(scenario) ? geometry() : [],
      ...occlusion(),
      ...paintOnly(),
      ...layerRoles(),
      ...oneSurface(),
    ];
    for (const [slot, { selector }] of Object.entries(slots).filter(([, { catalogue }]) => catalogue)) for (const words of document.querySelectorAll(selector))
      if (words.textContent && !declared.some((pattern) => pattern.test(plain(words.textContent!)))) found.push(`[C24 words] ${slot} shows a message it does not declare: "${words.textContent}"`);
    failures.push(...found.map((failure) => `${scenario}: ${failure}`));
    if (heldScreens.includes(scenario)) {
      const held = await whileHeld(() => document.documentElement.hasAttribute("data-keys-held") ? [...heldKeys(), ...occlusion()] : ["[C24 held] the hold drew nothing"]);
      failures.push(...held.map((failure) => `${scenario} (held): ${failure}`));
    }
    if (scenario === "results") idle.boxes = anchorBoxes();
    if ((frameStates as readonly string[]).includes(scenario)) states.set(scenario, anchorBoxes());
  }
  if (idle.boxes) for (const [scenario, boxes] of states) failures.push(...anchorMoves(idle.boxes, boxes, scenario));
  return failures;
}
