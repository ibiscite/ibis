import { afterEach, expect, test } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { IbisMark } from "./IbisMark";

afterEach(cleanup);

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
    expect([...svg.querySelectorAll("linearGradient stop")].map((node) => node.getAttribute("stop-color"))).toEqual([
      "light-dark(#CF4628, #FF7F5E)", "light-dark(#B5331A, #F2552F)",
    ]);
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
