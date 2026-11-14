import { cleanup, render } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Button } from "../src/ui/Button";
import { IconButton } from "../src/ui/IconButton";
import { colour } from "../src/ui/contrast";
import { say } from "../src/i18n";
import { contrast } from "./contrast";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

test.each(["text", "icon", "focus"])("painted %s pairs pass and a planted low-contrast pair fails", (kind) => {
  // jsdom supplies the real cascade but has no canvas rasterizer or layout.
  // Supply only those browser seams, keeping the real parts and pair collector.
  const context = {
    fillStyle: "#000", clearRect() {}, fillRect() {},
    getImageData() {
      const value = this.fillStyle.toLowerCase() === "canvas" ? "#fff" : this.fillStyle;
      const c = colour(value, new Map([["--ring", getComputedStyle(document.documentElement).getPropertyValue("--ring").trim()]]), false);
      return { data: new Uint8ClampedArray([c[0], c[1], c[2], c[3] * 255]) };
    },
  };
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(context as unknown as CanvasRenderingContext2D);
  vi.spyOn(Element.prototype, "getClientRects").mockReturnValue([new DOMRect(0, 0, 32, 32)] as unknown as DOMRectList);
  const sheet = document.createElement("style");
  const normal = ':root { --ring: #000; } body { background: #fff; color: #000; } .ui-button { color: #000; background: #fff; }';
  sheet.textContent = normal;
  document.head.append(sheet);
  try {
    const { container } = render(<><Button label={say("about-check")} /><IconButton aria-label={say("picker-more")} icon="chevrons" /></>);
    expect(contrast(container)).toEqual([]);
    sheet.textContent += kind === "text" ? ".ui-button { color: rgb(0 0 0 / 20%); }"
      : kind === "icon" ? ".ui-icon { color: #eee; }" : ":root { --ring: #eee; }";
    const errors = contrast(container);
    expect(errors.length).toBeGreaterThan(0);
    expect(errors.some((error) => error.includes(kind))).toBe(true);
  } finally { sheet.remove(); }
});
