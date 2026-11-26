import type { RevealTarget } from "./reveal";
/** A setting's current choice, before its reset or secondary actions. Radio
 * groups keep their selected input as the single keyboard stop. */
export function settingControl(row: HTMLElement): HTMLElement | null {
  return row.querySelector<HTMLElement>('input:checked:not(:disabled)')
    ?? row.querySelector<HTMLElement>(':is(input, button, select, [tabindex="0"]):not(:disabled):not([data-setting-reset])');
}

/** A setting found by search (ui/reveal): ready once its row is drawn, not
 * busy, and its control can take the keyboard. A disabled control is still
 * loading or unavailable; it must not hand search focus to its reset button. */
export function settingTarget(id: string): RevealTarget {
  return {
    find: (page) => {
      const label = document.getElementById(id);
      const row = label?.closest<HTMLElement>(".ui-row, tr, .ui-about-check") ?? label;
      if (!row || !page.contains(row) || row.closest("[hidden], [inert]") || row.getAttribute("aria-busy") === "true") return null;
      return !settingControl(row) && row.querySelector("input, button, select") ? null : row;
    },
    focus: settingControl,
  };
}
