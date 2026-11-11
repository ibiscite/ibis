import { useRef } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FormatMenu } from "./FormatMenu";

afterEach(cleanup);

function Example({ projects = () => undefined }: { projects?: () => void }) {
  const search = useRef<HTMLInputElement>(null);
  return <><input ref={search} aria-label="Search" /><FormatMenu app="Example Browser" format="autocite" source="remembered" pick="app"
    writing={{ name: "Thesis" }} onWritings={projects} onChange={() => undefined} disabled={false} styles searchRef={search} /></>;
}

test("the selected project stays visible and the format menu explains where changes go", () => {
  render(<Example />);
  const format = screen.getByRole("button", { name: /^Copy as .*Thesis$/ });
  expect(format.textContent).toContain("Thesis");
  expect(format.textContent).not.toContain("Example Browser");
  fireEvent.click(format);
  expect(screen.getByRole("menu").textContent).toContain("New writing…");
  expect(screen.getAllByText(/Saved for .*Thesis/).length).toBeGreaterThan(0);
  expect(screen.getByText(/Your pick changes this writing’s format/)).toBeTruthy();
});

test("Writings opens its sheet without choosing a citation format", () => {
  const projects = vi.fn();
  render(<Example projects={projects} />);
  fireEvent.click(screen.getByRole("button", { name: /^Copy as/ }));
  fireEvent.click(screen.getByRole("menuitem", { name: "New writing…" }));
  expect(projects).toHaveBeenCalledOnce();
  expect(screen.queryByRole("menu")).toBeNull();
});
