import { render } from "@testing-library/react";
import type { DisplayNode } from "../bindings";
import { renderDisplay } from "./display";

test("a hanging-indent bibliography renders from display blocks alone, keeping resets and literal hostile text", () => {
  const entry = (item: number): DisplayNode => ({
    item,
    content: [
      { text: `[${item + 1}]`, italic: false, bold: false, script: "normal" },
      { text: "Invented & ", italic: true, bold: false, script: "normal" },
      { text: "<script>literal</script><img src=x onerror=bad()>", italic: false, bold: false, script: "normal" },
      { text: " Tail", italic: true, bold: false, script: "normal", small_caps: true, underline: true },
    ],
    blocks: [
      { kind: "entry", start: 0, end: 4 },
      { kind: "margin", start: 0, end: 1 },
      { kind: "inline", start: 1, end: 4 },
      { kind: "indent", start: 1, end: 4 },
    ],
  });
  const { container } = render(<div>{renderDisplay({ nodes: [entry(0), entry(1)] })}</div>);
  const entries = container.querySelectorAll(".csl-entry");
  expect(entries).toHaveLength(2);
  expect(entries[0]?.querySelector(":scope > .csl-margin")?.textContent).toBe("[1]");
  expect(entries[0]?.querySelector(":scope > .csl-inline > .csl-indent")?.textContent).toBe("Invented & <script>literal</script><img src=x onerror=bad()> Tail");
  expect(entries[1]?.querySelector(".csl-margin")?.textContent).toBe("[2]");
  expect(container.querySelector("script, img, [onerror]")).toBeNull();
  expect(entries[0]?.querySelectorAll("i")).toHaveLength(2);
  expect(entries[0]?.querySelector(".csl-indent > span:nth-child(2) > i")).toBeNull();
});
