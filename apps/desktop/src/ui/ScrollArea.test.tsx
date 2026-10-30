import { act, cleanup, render } from "@testing-library/react";
import { useRef } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { KeysDialog } from "../features/picker/KeysDialog";
import { Notices } from "../features/picker/Notices";
import { markScrollEdges, useScrollEdges } from "./ScrollArea";
const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const css = (file: string) => readFileSync(new FileURL(file, import.meta.url), "utf8");

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

function Box() {
  const ref = useRef<HTMLDivElement>(null);
  useScrollEdges(ref);
  return <div ref={ref} className="scroll-area" data-testid="box"><p>One</p></div>;
}

test("a scroll area marks the edges with more content beyond them, as it scrolls and its content changes", async () => {
  const { getByTestId } = render(<Box />);
  const box = getByTestId("box");
  let height = 100, client = 100;
  Object.defineProperty(box, "scrollHeight", { get: () => height });
  Object.defineProperty(box, "clientHeight", { get: () => client });
  const marks = () => [box.hasAttribute("data-more-above"), box.hasAttribute("data-more-below")];
  fireScroll(box);
  expect(marks()).toEqual([false, false]);
  // More content arrives: the bottom edge has rows beyond it.
  height = 300;
  await act(async () => { box.append(document.createElement("p")); await Promise.resolve(); });
  expect(marks()).toEqual([false, true]);
  box.scrollTop = 100;
  fireScroll(box);
  expect(marks()).toEqual([true, true]);
  box.scrollTop = 200;
  fireScroll(box);
  expect(marks()).toEqual([true, false]);
  client = 300;
  box.scrollTop = 0;
  fireScroll(box);
  expect(marks()).toEqual([false, false]);
});

function fireScroll(node: HTMLElement) {
  act(() => { node.dispatchEvent(new Event("scroll")); });
}

test("the results, menus, Shortcuts and notices share the scroll area, and no screen sets its own overflow", () => {
  const styles = css("../styles.css");
  for (const selector of [".results-area", ".keys-scroll", "main[data-selection] .notices", "main[data-selection] .results-area"]) {
    const rule = styles.split("\n").find((line) => line.startsWith(`${selector} {`)) ?? "";
    expect(rule, selector).not.toBe("");
    expect(rule, selector).not.toMatch(/overflow|scrollbar/);
  }
  expect(css("./Menu.css")).not.toMatch(/\[role="menu"\] \{[^}]*overflow/);
  const keys = render(<KeysDialog bindings={[{ region: "picker", action: "pin", keys: ["CmdOrCtrl+Shift+P"] }]} isMac onClose={vi.fn()} />);
  expect(keys.container.querySelector(".keys-scroll")?.classList.contains("scroll-area")).toBe(true);
  const notices = render(<Notices library={null} failures={{}} startAtLogin={null} error="Search failed" copyError={null} copied={null} showCopiedText={false} onDismissCopied={vi.fn()} />);
  expect(notices.container.querySelector(".notices")?.classList.contains("scroll-area")).toBe(true);
});

test("the fade is valid at either edge and every system shares the native scrollbar", () => {
  const area = css("./ScrollArea.css");
  expect(area).toMatch(/\.scroll-area \{ overflow: auto; scrollbar-width: var\(--scrollbar-width\); scrollbar-color: var\(--scrollbar-color\);/);
  for (const edge of ["\\[data-more-above\\]", "\\[data-more-below\\]", "\\[data-more-above\\]\\[data-more-below\\]"]) {
    const rule = area.match(new RegExp(`\\.scroll-area${edge} \\{([^}]*)\\}`))?.[1] ?? "";
    expect(rule).toMatch(/-webkit-mask-image: linear-gradient\(/);
    expect(rule).toMatch(/[^-]mask-image: linear-gradient\(/);
    expect(rule).not.toMatch(/var\(--[\w-]+, 0\)/);
  }
  const tokens = css("../tokens.css");
  expect(tokens).toMatch(/--scrollbar-width: auto; --scrollbar-color: auto; --scroll-fade-size: var\(--space-4\);/);
  expect(tokens).not.toMatch(/--scrollbar-width: thin/);
});


test("fades yield to the entire active row without changing the user's scroll position", () => {
  const box = document.createElement("div"), row = document.createElement("div");
  box.append(row); document.body.append(box); row.dataset.focused = "true";
  Object.defineProperty(box, "clientHeight", { value: 100 });
  Object.defineProperty(box, "scrollHeight", { value: 300 });
  vi.spyOn(box, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 0, 200, 100));
  let top = 60, height = 40;
  vi.spyOn(row, "getBoundingClientRect").mockImplementation(() => new DOMRect(0, top, 200, height));
  box.scrollTop = 50;
  markScrollEdges(box);
  expect(box.style.getPropertyValue("--scroll-fade-below-space")).toBe("0px");
  expect(box.style.getPropertyValue("--scroll-fade-above-space")).toBe("60px");
  top = 0; height = 28; markScrollEdges(box);
  expect(box.style.getPropertyValue("--scroll-fade-above-space")).toBe("0px");
  expect(box.style.getPropertyValue("--scroll-fade-below-space")).toBe("72px");
  top = -10; height = 120; markScrollEdges(box);
  expect(box.style.getPropertyValue("--scroll-fade-above-space")).toBe("0px");
  expect(box.style.getPropertyValue("--scroll-fade-below-space")).toBe("0px");
  expect(box.scrollTop).toBe(50);
  row.removeAttribute("data-focused"); markScrollEdges(box);
  expect(box.style.getPropertyValue("--scroll-fade-below-space")).toBe("");
  box.scrollTop = 0; markScrollEdges(box);
  expect(box.hasAttribute("data-more-above")).toBe(false);
  box.scrollTop = 200; markScrollEdges(box);
  expect(box.hasAttribute("data-more-below")).toBe(false);
  box.remove();
});
