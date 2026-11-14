/** DOM order is visual order in shared parts. Roving collections expose only
 * their current item; hidden controls and negative Tab indices never join. */
export function tabStops(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>('button, input, select, textarea, a[href], [tabindex]')]
    .filter((node) => node.tabIndex >= 0 && !node.matches(":disabled")
      && !node.closest('[hidden], [inert], [aria-hidden="true"]')
      && visible(node));
}

function visible(node: HTMLElement): boolean {
  for (let at: HTMLElement | null = node; at; at = at.parentElement) {
    const style = getComputedStyle(at);
    if (style.display === "none" || style.visibility === "hidden") return false;
  }
  return true;
}
