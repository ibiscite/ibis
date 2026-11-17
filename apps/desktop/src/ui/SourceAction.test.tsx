import { iconBody } from "../testing/icons";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { SourceAction, shortenedLabel } from "./SourceAction";

const nativeIcon = { mime: "image/png", bytes: [137, 80, 78, 71] };

test("a native source icon is decorative; decode failure retains the named, working action", () => {
  const open = vi.fn();
  render(<SourceAction label="Open Zotero" icon={nativeIcon} open={open} />);
  const button = screen.getByRole("button", { name: "Open Zotero" });
  const image = button.querySelector("img")!;
  expect(image.alt).toBe("");
  expect(image.src).toBe("data:image/png;base64,iVBORw==");
  fireEvent.error(image);
  expect(button.querySelector("img")).toBeNull();
  expect(button.querySelector("svg")?.innerHTML).toBe(iconBody("app-window"));
  expect(button.querySelector("svg")?.innerHTML).not.toBe(iconBody("library-big"));
  fireEvent.click(button);
  expect(open).toHaveBeenCalledOnce();
});

afterEach(() => { vi.useRealTimers(); });

test("long names shorten in the middle, keep their extension and name the whole file", () => {
  const label = `Find ${"invented-thesis-references-".repeat(3)}final.bib`;
  expect(shortenedLabel(label).length).toBe(44);
  expect(shortenedLabel(label).endsWith("….bib")).toBe(true);
  expect(shortenedLabel(label).startsWith("Find invented")).toBe(true);
  expect(shortenedLabel("Find invented.bib")).toBe("Find invented.bib");
  render(<SourceAction label={label} icon={null} file open={() => {}} />);
  const action = screen.getByRole("button", { name: label });
  expect(action.getAttribute("title")).toBe(label);
  expect(action.textContent).toBe(shortenedLabel(label));
});

