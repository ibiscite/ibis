// Drives the guards of canvas/geometry.ts through the canvas scenarios in one
// page, so the runners (gallery/webkit.swift, gallery/canvas.mjs) only choose a
// window size and an appearance and print what this returns. Invented data only.
import { englishMessages, type MessageId } from "../src/i18n";
import { anchorBoxes, anchorMoves, frameStates, geometry, layerRoles, occlusion, overflow, paintOnly, settle, slots } from "./geometry";

type Mount = (host: HTMLElement, options: { scenario: string; theme: string }) => () => void;

/** Picker screens held to the single-picture layout rules (geometry()). */
export const geometryScreens = ["results", "first-run", "shelf", "reference-details", "show-as-menu", "reader", "commands", "page-in-search", "peek", "update", "undo-notice", "larger-text", "right-to-left", "compact", "long-titles", "stale", "more", "format-menu", "library-menu"];

/** Scenarios with something floating, for the layer check in every appearance. */
export const layerScreens = ["more", "format-menu", "library-menu", "show-as-menu", "reference-details", "peek", "shortcuts", "writing-projects", "cited-since-opening"];

/** Each catalogue message a slot can show, with the longest invented values its
 * callers pass. A message added to a slot's caller is added here. */
const tile = "Featherstonehaugh 2026";
export const slotMessages: Record<keyof typeof slots, { id: MessageId; args?: Record<string, string | number>; undo?: true }[]> = {
  footerNotice: [
    { id: "undo-removed", args: { reference: tile }, undo: true },
    { id: "picker-window-reset", undo: true },
    { id: "picker-undo-failed", undo: true },
    { id: "delight-tip", args: { keys: "⇧⌘P" } },
    { id: "delight-library100" }, { id: "delight-library1000" }, { id: "delight-library10000" },
    { id: "delight-empty" }, { id: "delight-loading" }, { id: "delight-no-library" },
    { id: "delight-zoom", args: { percent: 200 } },
    { id: "picker-shorthand-hint" },
    { id: "update-failed" },
  ],
};

/** Every guard failure in the scenarios this pass covers, as "scenario: failure".
 * frame: every scenario at the default window, plus the anchors across states;
 * narrow: words in their slots and nothing covered, at the narrowest window;
 * short: More in a window so short its list scrolls [C9];
 * layers: the floating surfaces only, in each system and Ibis appearance. */
export async function audit(host: HTMLElement, mount: Mount, scenarios: readonly string[], { pass = "frame", theme = "light" }: { pass?: "frame" | "narrow" | "short" | "layers"; theme?: string } = {}): Promise<string[]> {
  const failures: string[] = [];
  const show = async (scenario: string) => { mount(host, { scenario, theme }); await settle(); };
  const picker = scenarios.filter((scenario) => !["notice", "states", "tokens", "parts"].includes(scenario));

  if (pass === "short") {
    await show("more");
    return geometry().map((failure) => `more (short): ${failure}`);
  }

  if (pass === "layers") {
    for (const scenario of layerScreens.filter((name) => scenarios.includes(name))) {
      await show(scenario);
      failures.push(...layerRoles().map((failure) => `${scenario}: ${failure}`));
    }
    return failures;
  }

  if (pass === "narrow") {
    for (const scenario of picker) {
      await show(scenario);
      failures.push(...occlusion().map((failure) => `${scenario} (narrow): ${failure}`));
    }
    await show("undo-notice");
    for (const [slot, messages] of Object.entries(slotMessages) as [keyof typeof slots, typeof slotMessages[keyof typeof slots]][]) {
      const words = document.querySelector<HTMLElement>(slots[slot].selector);
      if (!words) { failures.push(`[C24 words] ${slot}: no slot drawn to measure`); continue; }
      const band = words.parentElement!, undo = band.querySelector<HTMLElement>(".ui-button");
      const original = words.textContent;
      for (const { id, args, undo: offer } of messages) {
        words.textContent = englishMessages.text(id, args);
        // A remark has the whole line; an offer shares it with Undo.
        if (undo) undo.hidden = !offer;
        const problem = overflow(words, slots[slot].lines);
        if (problem) failures.push(`[C24 words] ${slot} "${id}": ${problem} at ${document.documentElement.clientWidth}px`);
      }
      words.textContent = original;
      if (undo) undo.hidden = false;
    }
    return failures;
  }

  // A message no slot declares is caught wherever a scenario shows it.
  const declared = new Set(Object.values(slotMessages).flat().map(({ id, args }) => englishMessages.text(id, args)));
  const idle: { boxes?: ReturnType<typeof anchorBoxes> } = {};
  const states = new Map<string, ReturnType<typeof anchorBoxes>>();
  for (const scenario of scenarios.filter((name) => name !== "notice")) {
    await show(scenario);
    const found = [
      ...geometryScreens.includes(scenario) ? geometry() : [],
      ...occlusion(),
      ...paintOnly(),
      ...layerRoles(),
    ];
    for (const [slot, { selector }] of Object.entries(slots)) for (const words of document.querySelectorAll(selector))
      if (words.textContent && !declared.has(words.textContent)) found.push(`[C24 words] ${slot} shows a message it does not declare: "${words.textContent}"`);
    failures.push(...found.map((failure) => `${scenario}: ${failure}`));
    if (scenario === "results") idle.boxes = anchorBoxes();
    if ((frameStates as readonly string[]).includes(scenario)) states.set(scenario, anchorBoxes());
  }
  if (idle.boxes) for (const [scenario, boxes] of states) failures.push(...anchorMoves(idle.boxes, boxes, scenario));
  return failures;
}
