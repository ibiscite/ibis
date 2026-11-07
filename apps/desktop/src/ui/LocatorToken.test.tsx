import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { LocatorToken } from "./LocatorToken";

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
});
