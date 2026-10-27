import { render as show } from "@testing-library/react";
import { parse, render } from "./markup";

// Invented engine HTML exercises this UI boundary without a second formatter.
const hostile = '<div class="csl-entry"><div class="csl-left-margin">[1]</div><div class="csl-right-inline">A. Garcia, “A <b>bold</b> <span style="font-variant:small-caps">caps</span> x<sup>2</sup> &lt;script&gt;alert(1)&lt;/script&gt;&lt;img src=x onerror=alert(1)&gt;”</div></div>';

describe("formatted markup", () => {
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
    const html = hostile;
    const { container } = show(<div>{render(parse(html))}</div>);
    expect(container.querySelector("script, img, a, [onerror]")).toBeNull();
    expect(container.textContent).toContain("<script>alert(1)</script><img src=x onerror=alert(1)>");
    expect(container.querySelector("b")?.textContent).toBe("bold");
    expect(container.querySelector("span")?.getAttribute("style")).toBe("font-variant: small-caps;");
  });

});
