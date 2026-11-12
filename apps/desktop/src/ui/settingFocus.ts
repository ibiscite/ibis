/** A setting's current choice, before its reset or secondary actions. Radio
 * groups keep their selected input as the single keyboard stop. */
export function settingControl(row: HTMLElement): HTMLElement | null {
  return row.querySelector<HTMLElement>('input:checked:not(:disabled)')
    ?? row.querySelector<HTMLElement>('input:not(:disabled), button:not(:disabled):not([data-setting-reset]), select:not(:disabled), [tabindex="0"]');
}
