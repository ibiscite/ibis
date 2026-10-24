import { render as show } from "@testing-library/react";
import { Formatter, type CslItem } from "./engine";
import { clipboardHtml, parse, parseSegments, plainText, render } from "./markup";

// Synthetic reference whose title tries to inject markup.
const hostile: CslItem = {
  id: "hostile",
  type: "article-journal",
  title: "A <b>bold</b> <sc>caps</sc> x<sup>2</sup> <script>alert(1)</script><img src=x onerror=alert(1)>",
  author: [{ family: "Garcia", given: "Ana" }],
  issued: { "date-parts": [[2021]] },
  "container-title": "Journal of Tests",
};

describe("formatted markup", () => {
  const formatter = new Formatter();

  it("keeps only the markup the engine emits", () => {
    const markup = parse(
      '<div class="csl-entry" onclick="x()"><i>J</i> <span style="font-variant:small-caps;color:red">sc</span>'
        + '<script>alert(1)</script><img src=x onerror="alert(1)"><a href="javascript:x">link</a><style>p{}</style></div>',
    );
    expect(markup).toEqual([
      {
        block: "entry",
        children: [
          { tag: "i", children: ["J"] },
          " ",
          { tag: "span", style: ["font-variant:small-caps"], children: ["sc"] },
          "link",
        ],
      },
    ]);
  });

  it("ignores class names that only exist on the object prototype", () => {
    expect(parse('<div class="constructor __proto__ toString">x</div>')).toEqual(["x"]);
  });

  it("shows a hostile title as text in the preview", () => {
    const html = formatter.bibliography("apa", [hostile], "html").output;
    const { container } = show(<div>{render(parse(html))}</div>);
    expect(container.querySelector("script, img, a, [onerror]")).toBeNull();
    expect(container.textContent).toContain("<script>alert(1)</script><img src=x onerror=alert(1)>");
    expect(container.querySelector("b")?.textContent).toBe("bold");
    expect(container.querySelector("span")?.getAttribute("style")).toBe("font-variant: small-caps;");
  });

  it("writes clipboard HTML with inline styles and no margin blocks", () => {
    const html = formatter.bibliography("ieee", [hostile], "html").output;
    const copied = clipboardHtml(parse(html));
    expect(copied.startsWith('<p style="margin:0">[1] A. Garcia, “A <b>bold</b> ')).toBe(true);
    expect(copied).toContain('<span style="font-variant:small-caps">caps</span> x<sup>2</sup>');
    expect(copied).toContain("&lt;script&gt;alert(1)&lt;/script&gt;&lt;img src=x onerror=alert(1)&gt;");
    expect(copied).not.toContain("<div");
    expect(copied).not.toContain("class=");
  });

  it("writes one paragraph per entry", () => {
    const other = { ...hostile, id: "other", title: "Second" };
    const copied = clipboardHtml(parse(formatter.bibliography("apa", [hostile, other], "html").output));
    expect(copied.match(/<p /g)).toHaveLength(2);
  });

  it("keeps sanitized styles and literal separators in owned segments", () => {
    const parts = parseSegments('(<ibis-reference data-item="0"><i onclick="x()">García</i></ibis-reference>, '
      + '<ibis-reference data-item="0"><b>2021</b></ibis-reference>; '
      + '<ibis-reference data-item="1"><a href="javascript:x">سالم</a></ibis-reference>)', 2)!;
    expect(parts.map((part) => [part.item, plainText(part.markup)])).toEqual([
      [null, "("], [0, "García, 2021"], [null, "; "], [1, "سالم"], [null, ")"],
    ]);
    const { container } = show(<div>{parts.flatMap((part) => render(part.markup))}</div>);
    expect(container.querySelector("a, [onclick], ibis-reference")).toBeNull();
    expect(container.querySelector("i")?.textContent).toBe("García");
    expect(container.querySelector("b")?.textContent).toBe("2021");
  });

  it.each(["-1", "01", "1.0", "1e0", "Infinity", "", "2", "9007199254740993"])("rejects invalid owner %s", (item) => {
    expect(parseSegments(`<ibis-reference data-item="${item}">text</ibis-reference>`, 2)).toBeNull();
  });

  it("rejects extra ownership attributes and contradictory nesting", () => {
    expect(parseSegments('<ibis-reference data-item="0" onclick="x()">text</ibis-reference>', 1)).toBeNull();
    expect(parseSegments('<ibis-reference data-item="0"><ibis-reference data-item="1">text</ibis-reference></ibis-reference>', 2)).toBeNull();
  });

  it("leaves interleaved ownership opaque rather than merging unrelated references", () => {
    const html = '<ibis-reference data-item="0">A</ibis-reference>, '
      + '<ibis-reference data-item="1">B</ibis-reference>, '
      + '<ibis-reference data-item="0">C</ibis-reference>, '
      + '<ibis-reference data-item="2">D</ibis-reference>';
    const parts = parseSegments(html, 3)!;
    expect(parts.map((part) => [part.item, plainText(part.markup)])).toEqual([[null, "A, B, C, "], [2, "D"]]);
  });

  it("drops executable content and bounds annotation parsing", () => {
    const html = '<script>bad()</script><style>bad{}</style><template>bad</template><ibis-reference data-item="0">safe</ibis-reference>';
    expect(parseSegments(html, 1)).toEqual([{ item: 0, markup: ["safe"] }]);
    expect(parseSegments("x", -1)).toBeNull();
    expect(parseSegments("x", 1001)).toBeNull();
    expect(parseSegments("x".repeat(1024 * 1024 + 1), 1)).toBeNull();
    expect(parseSegments("<span>".repeat(257) + "x" + "</span>".repeat(257), 1)).toBeNull();
    expect(parseSegments("x<i>x</i>".repeat(5001), 1)).toBeNull();
  });
});
