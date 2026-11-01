import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { LocatorToken } from "./LocatorToken";

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
afterEach(cleanup);

test.each(["Enter", " "])("a long mixed-script locator retains its full name and removes once with %s", (key) => {
  const value = `section ${"研究 אבג <script>".repeat(100)}`;
  const remove = vi.fn();
  const { container } = render(<LocatorToken text={value} removeLabel={`Remove ${value} from a`} onRemove={remove} />);
  const button = screen.getByRole("button", { name: `Remove ${value} from a` });
  expect(container.querySelector("bdi")?.textContent).toBe(value);
  expect(container.querySelector("script")).toBeNull();
  expect(container.firstElementChild?.getAttribute("title")).toBe(value);
  act(() => button.focus());
  fireEvent.keyDown(button, { key, code: key === " " ? "Space" : "Enter" });
  fireEvent.keyUp(button, { key, code: key === " " ? "Space" : "Enter" });
  expect(remove).toHaveBeenCalledOnce();
  const css = readFileSync(new FileURL("./LocatorToken.css", import.meta.url), "utf8");
  expect(css).toContain("min-inline-size: 0");
  expect(css).toContain("max-inline-size: 50%");
  expect(css).toContain("text-overflow: ellipsis");
  expect(css).toContain("--locator-token-hit-size: var(--control-h)");
  expect(css).toContain("--locator-token-hit-size: var(--touch-target)");
  expect(css).toContain("padding-inline-end: calc(var(--locator-token-hit-size) - var(--control-small-h))");
});
