import { iconBody } from "./testing/icons";
import { expect, test } from "vitest";
import { render } from "@testing-library/react";
import { Icon, kindOf } from "./icons";
import { geometry } from "../canvas/geometry";
import { art } from "./icon-art";

test("control and lead-column icons draw the unchanged filled Phosphor glyph", () => {
  for (const size of ["control", "tile"] as const) {
    const view = render(<Icon name="check" size={size} />);
    const glyph = view.container.querySelector("svg")!;
    expect(glyph.getAttribute("viewBox")).toBe(art.viewBox);
    expect(glyph.getAttribute("fill")).toBe("currentColor");
    expect(glyph.innerHTML).toBe(iconBody("check"));
    expect(glyph.getAttribute("data-size")).toBe(size);
    expect(glyph.getAttribute("aria-hidden")).toBe("true");
    view.unmount();
  }
});

test("the canvas accepts 16 and 20 px glyphs and refuses other sizes", () => {
  const view = render(<Icon name="check" />);
  const glyph = view.container.querySelector("svg")!;
  for (const [pixels, failures] of [[16, 0], [20, 0], [14, 1]]) {
    // Browser measurements supplied at jsdom's SVG boundary.
    glyph.getBoundingClientRect = () => new DOMRect(0, 0, pixels, pixels);
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
