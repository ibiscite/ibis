import { expect, test } from "vitest";
import { render } from "@testing-library/react";
import { Icon, kindOf } from "./icons";
import { geometry } from "../canvas/geometry";

test("controls and tiles keep the same 1.5 px stroke on the Lucide grid", () => {
  for (const [size, pixels] of [["control", 16], ["tile", 20]] as const) {
    const view = render(<Icon name="check" size={size} />);
    const glyph = view.container.querySelector("svg")!;
    expect(glyph.getAttribute("viewBox")).toBe("0 0 24 24");
    expect(Number(glyph.getAttribute("stroke-width")) * pixels / 24).toBe(1.5);
    expect(glyph.getAttribute("data-size")).toBe(size);
    expect(glyph.getAttribute("aria-hidden")).toBe("true");
    expect(glyph.querySelectorAll("path")).toHaveLength(1);
    expect(glyph.querySelector("path")!.getAttribute("pathLength")).toBe("1");
    view.unmount();
  }
});

test("the canvas refuses a resized glyph or a scaled-down stroke", () => {
  const view = render(<Icon name="check" />);
  const glyph = view.container.querySelector("svg")!, path = glyph.querySelector("path")!;
  for (const [pixels, stroke, scale, failures] of [[16, 2.25, 2 / 3, 0], [20, 1.8, 5 / 6, 0], [14, 1.5, .7, 2], [16, 1.5, 2 / 3, 1]]) {
    // Browser measurements supplied at jsdom's SVG boundary.
    glyph.getBoundingClientRect = () => new DOMRect(0, 0, pixels, pixels);
    path.style.strokeWidth = String(stroke);
    path.getScreenCTM = () => ({ a: scale, b: 0, c: 0, d: scale }) as DOMMatrix;
    expect(geometry(view.container).filter((failure) => failure.startsWith("[icons]"))).toHaveLength(failures!);
  }
  view.unmount();
});

test("every CSL item type gets one of the seven type icons", () => {
  expect(["article-journal", "book", "chapter", "thesis", "webpage", "report", "dataset"].map(kindOf))
    .toEqual(["article", "book", "chapter", "thesis", "web", "report", "other"]);
  expect(kindOf("paper-conference")).toBe("article");
  expect(kindOf("entry-encyclopedia")).toBe("chapter");
  expect(kindOf("post-weblog")).toBe("web");
  // Unknown, missing, oddly cased or prototype-named types fall back to "other".
  for (const type of [undefined, "", "ARTICLE-JOURNAL", "article-journal\u0000", "__proto__", "constructor", "toString"]) {
    expect(kindOf(type)).toBe("other");
  }
});
