import { afterEach, expect, test } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { Progress } from "./Progress";

afterEach(() => { cleanup(); delete document.documentElement.dataset.platform; });

test.each(["macos", "windows", "linux"])("%s distinguishes completed downloads from indeterminate phases", (platform) => {
  document.documentElement.dataset.platform = platform;
  const { rerender } = render(<Progress label="Downloading update…" value={100} />);
  const bar = () => screen.getByRole("progressbar");
  const fill = () => bar().querySelector<SVGElement>(".update-ring-fill")!;
  expect(bar().getAttribute("aria-valuenow")).toBe("100");
  expect(bar().textContent).toContain("100%");
  expect(fill().style.getPropertyValue("--progress")).toBe("100");
  expect(bar().hasAttribute("data-indeterminate")).toBe(false);
  for (const label of ["Checking and installing update…", "Downloading update…"]) {
    rerender(<Progress label={label} />);
    expect(screen.getByRole("progressbar", { name: label })).toBeTruthy();
    expect(bar().hasAttribute("aria-valuenow")).toBe(false);
    expect(bar().textContent).not.toContain("%");
    expect(bar().hasAttribute("data-indeterminate")).toBe(true);
    expect(fill().style.getPropertyValue("--progress")).toBe("");
  }
  rerender(<Progress label="Downloading update…" value={0} />);
  expect(bar().getAttribute("aria-valuenow")).toBe("0");
  expect(fill().style.getPropertyValue("--progress")).toBe("0");
  expect(bar().hasAttribute("data-indeterminate")).toBe(false);
});
