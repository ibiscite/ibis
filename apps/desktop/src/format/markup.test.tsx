import { render as show } from "@testing-library/react";
import { Formatter, type CslItem } from "./engine";
import { clipboardHtml, parse, render } from "./markup";

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
});
