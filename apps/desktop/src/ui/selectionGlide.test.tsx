import { afterEach, expect, test } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { useRef } from "react";
import { useSelectionGlide } from "./selectionGlide";

afterEach(cleanup);

function Rows({ rows, active }: { rows: readonly string[]; active: number }) {
  const list = useRef<HTMLUListElement>(null);
  useSelectionGlide(list, active, rows);
  return <ul ref={list}>{rows.map((row, i) => <li key={row} data-active={i === active || undefined}
    ref={(node) => { if (node) { node.getBoundingClientRect = () => ({ top: i * 50 }) as DOMRect; Object.defineProperty(node, "offsetHeight", { value: 50 }); } }}>{row}</li>)}</ul>;
}

test("a highlight that moves down glides in from above, and new results never glide", () => {
  const rows = ["a", "b", "c"];
  const { rerender, getByText } = render(<Rows rows={rows} active={0} />);
  rerender(<Rows rows={rows} active={1} />);
  const b = getByText("b");
  expect(b.dataset.glide).toBe("");
  expect(b.style.getPropertyValue("--glide-from").startsWith("-")).toBe(true);
  rerender(<Rows rows={["x", "y"]} active={1} />);
  expect(getByText("y").dataset.glide).toBeUndefined();
  expect(document.querySelectorAll("[data-glide]")).toHaveLength(0);
});
