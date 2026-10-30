import { afterEach, expect, test, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { Progress } from "./Progress";

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const css = readFileSync(new FileURL("./Progress.css", import.meta.url), "utf8");
const tokens = readFileSync(new FileURL("../tokens.css", import.meta.url), "utf8");
afterEach(() => { cleanup(); delete document.documentElement.dataset.platform; });

test.each(["macos", "windows", "linux"])("%s distinguishes completed downloads from indeterminate phases", (platform) => {
  document.documentElement.dataset.platform = platform;
  const { rerender } = render(<Progress label="Downloading update…" value={100} />);
  const bar = () => screen.getByRole("progressbar");
  const fill = () => bar().querySelector<HTMLElement>(".update-track > div")!;
  expect(bar().getAttribute("aria-valuenow")).toBe("100");
  expect(bar().textContent).toContain("100%");
  expect(fill().style.width).toBe("100%");
  expect(bar().hasAttribute("data-indeterminate")).toBe(false);
  for (const label of ["Checking and installing update…", "Downloading update…"]) {
    rerender(<Progress label={label} />);
    expect(screen.getByRole("progressbar", { name: label })).toBeTruthy();
    expect(bar().hasAttribute("aria-valuenow")).toBe(false);
    expect(bar().textContent).not.toContain("%");
    expect(bar().hasAttribute("data-indeterminate")).toBe(true);
    expect(fill().style.width).toBe("");
  }
  rerender(<Progress label="Downloading update…" value={0} />);
  expect(bar().getAttribute("aria-valuenow")).toBe("0");
  expect(fill().style.width).toBe("0%");
  expect(bar().hasAttribute("data-indeterminate")).toBe(false);
});

test.each(["@media (prefers-contrast: more) {", "@media (forced-colors: active) {"])("progress keeps a contrasting system pair in %s", (mode) => {
  expect(tokens).toContain(mode);
  const block = tokens.slice(tokens.indexOf(mode)).split("\n}")[0]!;
  expect(block).toContain("--progress-track: Canvas;");
  expect(block).toContain("--progress-fill: CanvasText;");
  expect(block).toContain("--progress-border: CanvasText;");
  expect(block).toContain("--progress-border-width: var(--border-width);");
});

test("progress uses role colours, preserves them in forced colours", () => {
  expect(css).toContain("background: var(--progress-track)");
  expect(css).toContain("background: var(--progress-fill)");
  expect(css).toContain("border: var(--progress-border-width) solid var(--progress-border)");
  expect(css).toMatch(/forced-colors: active[^}]*forced-color-adjust: none/s);
});

test("indeterminate motion becomes a full-width dashed indicator with reduced motion", () => {
  expect(css).toMatch(/\[data-indeterminate\][^{]*\{[^}]*animation: progress-slide var\(--progress-duration\)/);
  expect(css).toMatch(/@keyframes progress-slide[^]*transform: translateX\(var\(--progress-travel\)\)/);
  const reduced = css.slice(css.indexOf("@media (prefers-reduced-motion: reduce)"));
  expect(reduced).toContain("animation: none");
  expect(reduced).toContain("width: 100%");
  expect(reduced).toContain("background: none");
  expect(reduced).toContain("border-block-start: var(--space-1) dashed var(--progress-fill)");
});

test("indeterminate progress and the opening mark pause while the picker is hidden", () => {
  const empty = readFileSync(new FileURL("../features/picker/EmptyState.css", import.meta.url), "utf8");
  expect(css).toMatch(/:root\[data-picker-hidden\] \.update-progress\[data-indeterminate\][^{]*\{ animation-play-state: paused; \}/);
  expect(empty).toMatch(/:root\[data-picker-hidden\] \.opening-mark[^}]*animation-play-state: paused;/);
});
