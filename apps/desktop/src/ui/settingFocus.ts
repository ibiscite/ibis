/** A setting's current choice, before its reset or secondary actions. Radio
 * groups keep their selected input as the single keyboard stop. */
export function settingControl(row: HTMLElement): HTMLElement | null {
  return row.querySelector<HTMLElement>('input:checked:not(:disabled)')
    ?? row.querySelector<HTMLElement>(':is(input, button, select, [tabindex="0"]):not(:disabled):not([data-setting-reset])');
}
