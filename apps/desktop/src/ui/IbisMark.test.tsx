import { afterEach, expect, test, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { IbisMark } from "./IbisMark";

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const css = readFileSync(new FileURL("./IbisMark.css", import.meta.url), "utf8");
let contrastStyle: HTMLStyleElement | undefined;
afterEach(() => { cleanup(); contrastStyle?.remove(); contrastStyle = undefined; });

test.each(["(prefers-contrast: more)", "(forced-colors: active)"])("all mark sizes use a flat CanvasText bird and preserve the eye in %s", (mode) => {
  const parser = document.createElement("style");
  parser.textContent = css;
  document.head.append(parser);
  const media = [...parser.sheet!.cssRules].find((rule) => rule instanceof CSSMediaRule && rule.conditionText.includes(mode)) as CSSMediaRule | undefined;
  parser.remove();
  expect(media, `Missing contrast mode ${mode}`).toBeDefined();
  // jsdom does not evaluate system media preferences. Activate the actual
  // matching CSS rules and check their cascade against the generated artwork.
  contrastStyle = document.createElement("style");
  contrastStyle.textContent = [...media!.cssRules].map((rule) => rule.cssText).join("\n");
  document.head.append(contrastStyle);
  const paints = [...media!.cssRules].filter((rule) => rule instanceof CSSStyleRule && (rule.style.fill || rule.style.stroke)) as CSSStyleRule[];
  expect(paints).toHaveLength(2);
  for (const rule of paints) expect((rule.style.fill || rule.style.stroke).toLowerCase()).toBe("canvastext");
  const { container } = render(<><span style={{ fill: "CanvasText", stroke: "CanvasText" }} /><IbisMark size="welcome" /><IbisMark /><IbisMark size="search" /></>);
  // System colours may compute to RGB. Compare against the same system paint,
  // while the declarations above verify that it follows CanvasText itself.
  const canvas = getComputedStyle(container.querySelector("span")!);
  expect(canvas.fill).toBeTruthy();
  for (const svg of container.querySelectorAll("svg")) {
    for (const shape of svg.querySelectorAll('g [fill]:not([fill="none"]), g [stroke]:not([stroke="none"])')) {
      const paint = shape.hasAttribute("fill") && shape.getAttribute("fill") !== "none" ? "fill" : "stroke";
      expect(getComputedStyle(shape).getPropertyValue(paint)).toBe(canvas.getPropertyValue(paint));
    }
    for (const group of svg.querySelectorAll("g[filter]")) expect(getComputedStyle(group).filter).toBe("none");
    const shade = svg.querySelector(".eyeshade");
    if (shade) expect(getComputedStyle(shade).display).toBe("none");
    const eye = svg.querySelector("mask circle")!;
    const mask = eye.parentElement!;
    for (const rule of paints) {
      expect(eye.matches(rule.selectorText)).toBe(false);
      expect(mask.querySelector("rect")!.matches(rule.selectorText)).toBe(false);
    }
    expect(eye.getAttribute("fill")).toBe("#000");
    expect(mask.querySelector("rect")?.getAttribute("fill")).toBe("#fff");
  }
});

test("welcome, opening and search marks keep distinct, resolvable gradient and eye ids", () => {
  const { container } = render(<><IbisMark size="welcome" /><IbisMark /><IbisMark size="search" /></>);
  const ids = [...container.querySelectorAll("[id]")].map((node) => node.id);
  expect(new Set(ids).size).toBe(ids.length);
  for (const svg of container.querySelectorAll("svg")) {
    expect(svg.getAttribute("aria-hidden")).toBe("true");
    expect(svg.getAttribute("focusable")).toBe("false");
    for (const node of svg.querySelectorAll("[mask], [fill], [stroke], [filter]")) {
      for (const attribute of ["mask", "fill", "stroke", "filter"]) {
        const reference = node.getAttribute(attribute)?.match(/^url\(#(.+)\)$/)?.[1];
        if (reference) expect([...svg.querySelectorAll("[id]")].some((target) => target.id === reference)).toBe(true);
      }
    }
  }
  // The welcome keeps the brand gradient; the smaller marks are flat --scarlet,
  // the same red as the shelf beak and the citation tile.
  const [welcome, ...small] = container.querySelectorAll("svg");
  expect([...welcome!.querySelectorAll("linearGradient stop")].map((node) => node.getAttribute("stop-color"))).toEqual([
    "light-dark(#CF4628, #FF7F5E)", "light-dark(#B5331A, #F2552F)",
  ]);
  for (const svg of small) {
    expect(svg.querySelector("linearGradient")).toBeNull();
    expect(svg.querySelector('[fill="currentColor"]')).toBeTruthy();
  }
});

test("the 24 px picker export opens its eye to two pixels without changing the silhouette", () => {
  const { container } = render(<><IbisMark /><IbisMark size="search" /></>);
  const [large, small] = [...container.querySelectorAll("svg")];
  const side = Number(small!.getAttribute("viewBox")!.split(" ")[2]);
  const eye = Number(small!.querySelector("mask circle")!.getAttribute("r"));
  expect(eye * 2 * 24 / side).toBeGreaterThanOrEqual(2);
  expect([...small!.querySelectorAll("g path")].map((node) => node.getAttribute("d")))
    .toEqual([...large!.querySelectorAll("g path")].map((node) => node.getAttribute("d")));
  expect(Number(large!.querySelector("mask circle")!.getAttribute("r"))).toBe(2.1);
});
