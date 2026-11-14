import { contrastFailures, over, type Colour, type ContrastPair } from "../src/ui/contrast";

/** The browser supplies the cascade, system colours and layout; this check sees
 * the pairs actually painted, including inherited ink, alpha and nested fills. */
export function contrast(root: HTMLElement = document.body): string[] {
  const context = document.createElement("canvas").getContext("2d", { willReadFrequently: true })!;
  const cache = new Map<string, Colour>();
  const rgb = (value: string): Colour => {
    let result = cache.get(value);
    if (!result) {
      context.clearRect(0, 0, 1, 1);
      context.fillStyle = value;
      context.fillRect(0, 0, 1, 1);
      const [r, g, b, a] = context.getImageData(0, 0, 1, 1).data;
      result = [r!, g!, b!, a! / 255]; cache.set(value, result);
    }
    return result;
  };
  const probe = document.createElement("span");
  probe.style.background = "Canvas";
  document.body.append(probe);
  const canvas = rgb(getComputedStyle(probe).backgroundColor);
  probe.remove();
  const painted = (node: Element, ink: Colour): Colour => {
    let pixel = ink;
    for (let at: Element | null = node; at; at = at.parentElement) {
      const style = getComputedStyle(at);
      pixel = over(pixel, rgb(style.backgroundColor));
      pixel = [pixel[0], pixel[1], pixel[2], pixel[3] * Number(style.opacity || 1)];
    }
    return over(pixel, canvas);
  };
  const pairs: ContrastPair[] = [];
  for (const node of root.querySelectorAll<HTMLElement>("*")) {
    if (!node.getClientRects().length || node.closest(".visually-hidden, [hidden]")) continue;
    const style = getComputedStyle(node);
    if (style.visibility !== "visible" || style.opacity === "0") continue;
    const name = `${node.tagName.toLowerCase()}.${[...node.classList].join(".")}`;
    const paper = painted(node, [0, 0, 0, 0]);
    const text = [...node.childNodes].some((child) => child.nodeType === Node.TEXT_NODE && child.textContent?.trim());
    const field = node instanceof HTMLInputElement && !node.matches('[type="checkbox"], [type="radio"], [type="hidden"]') ? node : null;
    if (text || field) {
      const ink = field && !field.value ? getComputedStyle(node, "::placeholder").color : style.color;
      pairs.push({ name: `${name} text ${ink}`, ink: painted(node, rgb(ink)), paper, kind: "text" });
    }
    if (node.matches("svg.ui-icon, .type-icon, .shelf-mark, .search-mark, .ui-status-mark, .status-dot, .update-ring-fill")) {
      const ink = node.matches(".status-dot, .ui-status-mark[data-presentation='dot']") ? style.backgroundColor : node.matches(".update-ring-fill") ? style.stroke : style.color;
      pairs.push({ name: `${name} icon ${ink}`, ink: painted(node, rgb(ink)), paper: node.matches(".status-dot, .ui-status-mark[data-presentation='dot']") ? painted(node.parentElement!, [0, 0, 0, 0]) : paper, kind: "icon" });
    }
    if (node.tabIndex >= 0 && !node.matches(":disabled")) {
      // The shared ring is outside a control; its surround is its parent's fill.
      probe.style.color = "var(--ring)";
      node.parentElement!.append(probe);
      const ring = rgb(getComputedStyle(probe).color);
      probe.remove();
      pairs.push({ name: `${name} focus`, ink: painted(node.parentElement!, ring), paper: painted(node.parentElement!, [0, 0, 0, 0]), kind: "focus" });
    }
  }
  return [...new Set(contrastFailures(pairs))];
}
