import { useRef } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { FormatMenu } from "./FormatMenu";

afterEach(cleanup);

function Example({ writing, reset }: { writing?: string; reset: () => void }) {
  const search = useRef<HTMLInputElement>(null);
  return <><input ref={search} aria-label="Search" /><FormatMenu app="Word" format="modern-language-association" source="remembered" pick="app"
    writing={writing ? { name: writing } : null} usualFormat="apa" onUseUsual={reset}
    onWritings={() => undefined} onChange={() => undefined} disabled={false} styles searchRef={search} /></>;
}

test("the app format offers to use the usual format again", () => {
  const reset = vi.fn();
  render(<Example reset={reset} />);
  fireEvent.click(screen.getByRole("button", { name: /^Citation format/ }));
  fireEvent.click(screen.getByRole("option", { name: "Use APA in Word again" }));
  expect(reset).toHaveBeenCalledOnce();
});

test("a writing offers to stop using it in the captured app", () => {
  const reset = vi.fn();
  render(<Example writing="Thesis" reset={reset} />);
  fireEvent.click(screen.getByRole("button", { name: /^Citation format/ }));
  fireEvent.click(screen.getByRole("option", { name: "Stop using Thesis in Word" }));
  expect(reset).toHaveBeenCalledOnce();
  expect(screen.queryByText("Use APA in Word again")).toBeNull();
});
