import { render } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { PickerPicture } from "./PickerPicture";

// Vitest stubs CSS modules by default; these previews intentionally consume raw
// component styles, so exercise the actual bundled bytes inside their shadows.
const sources = await vi.hoisted(async () => {
  const { readFileSync } = await vi.importActual<{ readFileSync: (url: URL, encoding: string) => string }>("node:fs");
  const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
  return { picker: readFileSync(new FileURL("../styles.css", import.meta.url), "utf8"), button: readFileSync(new FileURL("../ui/Button.css", import.meta.url), "utf8") };
});
vi.mock("../styles.css?raw", () => ({ default: sources.picker }));
vi.mock("../ui/Button.css?raw", () => ({ default: sources.button }));

test.each(["light", "dark"])("the %s snapshot owns its theme at the shadow host and retains the real controls and list styling", (appearance) => {
  const { container } = render(<PickerPicture appearance={appearance} />);
  const host = container.querySelector<HTMLElement>(".ui-picker-picture-content > div")!;
  expect(host.style.colorScheme).toBe(appearance);
  expect(host.shadowRoot!.querySelector("style")!.textContent).toContain(".ui-button[data-variant]");
  expect(host.shadowRoot!.querySelector("style")!.textContent).not.toContain("color-scheme: light dark;");
  expect(host.shadowRoot!.querySelector(".results-area > #results")).toBeTruthy();
});

test("System has two explicitly themed snapshots and crops through the frame's diagonal", () => {
  const { container } = render(<PickerPicture appearance="light" split />);
  expect([...container.querySelectorAll<HTMLElement>(".ui-picker-picture-content > div")].map((host) => host.style.colorScheme)).toEqual(["light", "dark"]);
  expect(container.querySelector(".ui-picker-picture-mask[data-dark]")).toBeTruthy();
});
