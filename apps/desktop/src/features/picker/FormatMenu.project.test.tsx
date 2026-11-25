import { useRef } from "react";
import { afterEach, expect, test } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FormatMenu } from "./FormatMenu";

afterEach(cleanup);

function Example({ projects = () => undefined }: { projects?: () => void }) {
  const search = useRef<HTMLInputElement>(null);
  return <><input ref={search} aria-label="Search" /><FormatMenu app="Example Browser" format="autocite" source="remembered" pick="app"
    writing={{ name: "Thesis" }} onWritings={projects} onChange={() => undefined} disabled={false} styles searchRef={search} /></>;
}

test("the pop-up names the format alone; the project is its status and the palette's heading", () => {
  render(<Example />);
  const format = screen.getByRole("button", { name: /^Citation format / });
  expect(format.textContent).not.toContain("Thesis");
  expect(format.textContent).not.toContain("Example Browser");
  expect(screen.getByRole("status", { name: "Citation format" }).textContent).toContain("Thesis");
  fireEvent.click(format);
  expect(screen.getByRole("dialog").textContent).toContain("New project…");
  expect(screen.getAllByText(/Saved for .*Thesis/).length).toBeGreaterThan(0);
  expect(screen.getByText(/Your pick changes this project’s format/)).toBeTruthy();
});
