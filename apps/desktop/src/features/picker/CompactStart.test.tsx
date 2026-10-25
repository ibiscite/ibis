import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { CompactStart } from "./CompactStart";

test("compact start shows one tip at a time and its place in the round", () => {
  const { unmount } = render(<CompactStart />);
  const next = screen.getByRole("button", { name: /^Next tip \(\d of 6\)$/ });
  const first = next.textContent;
  const label = document.querySelector(".key-hint-label")?.textContent;
  fireEvent.click(next);
  expect(next.textContent).not.toBe(first);
  expect(document.querySelector(".key-hint-label")?.textContent).not.toBe(label);
  expect(document.querySelectorAll(".compact-start .key-hint")).toHaveLength(1);
  const shown = next.textContent;
  unmount();
  // The next start goes on from the tip after the last one shown.
  render(<CompactStart />);
  const [n] = (shown ?? "").split(" of ").map(Number);
  expect(screen.getByRole("button", { name: /^Next tip/ }).textContent).toBe(`${(n! % 6) + 1} of 6`);
});
