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

test("results, menus and Shortcuts share the scroll area; picker notices belong to the middle scroller", () => {
  const styles = css("../styles.css");
  for (const selector of [".results-area", ".keys-scroll", ".picker-content > .notices"]) {
    const rule = styles.split("\n").find((line) => line.startsWith(`${selector} {`)) ?? "";
    expect(rule, selector).not.toBe("");
    expect(rule, selector).not.toMatch(/overflow|scrollbar/);
  }
  expect(css("./Menu.css")).not.toMatch(/\[role="menu"\] \{[^}]*overflow/);
  const keys = render(<KeysDialog bindings={[{ region: "picker", action: "pin", keys: ["CmdOrCtrl+Shift+P"] }]} isMac onClose={vi.fn()} />);
  expect(keys.container.querySelector(".keys-scroll")?.classList.contains("scroll-area")).toBe(true);
  const notices = render(<Notices library={null} failures={{}} error="Search failed" copyError={null} copied={null} onDismissCopied={vi.fn()} />);
  expect(notices.container.querySelector(".notices")?.classList.contains("scroll-area")).toBe(false);
  const noticeRule = styles.split("\n").find((line) => line.startsWith(".picker-content > .notices {")) ?? "";
  expect(noticeRule).toMatch(/flex: none/);
  expect(noticeRule).not.toMatch(/max-height/);
});

test("nonempty notices hand results to the middle scroller independently of the middle's height", () => {
  // jsdom checks the notice-dependent ownership rule; picker-layout.browser.mjs
  // checks actual overflow with a large notice, selected references and results.
  const styles = css("../styles.css");
  const props = { library: null, failures: {}, error: null, copyError: null,
    onDismissCopied: vi.fn() };
  const warnings = Array.from({ length: 16 }, (_, i) => `Invented copy warning ${i + 1}`);
  const picker = (copied: { text: string; warnings: string[] } | null) => <main data-selection><div className="picker-content scroll-area">
    <Notices {...props} copied={copied} /><div className="results-area scroll-area" /><div className="shelf" />
  </div></main>;
  const view = render(picker({ text: "[@synthetic0]", warnings }));
  const results = view.container.querySelector(".results-area")!;
  const noticeRules = () => [...styles.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
    .filter(([, head]) => {
      const selector = head!.replace(/\/\*[^]*?\*\//g, "").trim();
      return selector.includes(".notices") && results.matches(selector);
    }).map(([, , body]) => body).join(";");
  const fallback = document.createElement("div").style;
  fallback.cssText = noticeRules();
  expect(fallback.flex).toBe("0 0 auto");
  expect(parseFloat(fallback.minHeight)).toBe(0);
  expect(fallback.overflow).toBe("visible");
  view.rerender(picker(null));
  expect(noticeRules()).toBe("");
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
  expect(tokens).toMatch(/--scrollbar-width: auto; --scrollbar-color: auto; --scroll-fade-size: 14px;/);
  expect(tokens).not.toMatch(/--scrollbar-width: thin/);
});

test.each(["normal", "prefers-contrast: more", "prefers-reduced-transparency: reduce", "forced-colors: active"])(
  "with both scroll edges marked, %s selects the correct mask for each browser property", (preference) => {
  const view = render(<div className="scroll-area" data-more-above data-more-below />);
  const node = view.container.firstElementChild!;
  const sheet = document.createElement("style");
  sheet.textContent = css("./ScrollArea.css");
  document.head.append(sheet);
  try {
    // jsdom does not evaluate media queries. Select the real applicable rules
    // and compare their specificity before source order. ScrollArea's selectors
    // use only classes and attributes, which each contribute one to specificity.
    const active = [...sheet.sheet!.cssRules].flatMap((rule) => {
      if (rule.type !== CSSRule.MEDIA_RULE) return [rule];
      const media = rule as CSSMediaRule;
      return media.conditionText.split(",").some((query) => query.trim() === `(${preference})`) ? [...media.cssRules] : [];
    }).filter((rule): rule is CSSStyleRule => rule.type === CSSRule.STYLE_RULE);
    const matching = active.flatMap((rule) => rule.selectorText.split(",").map((selector) => selector.trim())
      .filter((selector) => node.matches(selector)).map((selector) => {
        expect(selector).toMatch(/^\.scroll-area(?:\[[\w-]+\])*$/);
        return { style: rule.style, specificity: (selector.match(/\.[\w-]+|\[[\w-]+\]/g) ?? []).length };
      }));
    for (const property of ["-webkit-mask-image", "mask-image"]) {
      const winner = matching.filter(({ style }) => style.getPropertyValue(property))
        .sort((a, b) => a.specificity - b.specificity).at(-1);
      expect(winner, property).toBeTruthy();
      const value = winner!.style.getPropertyValue(property);
      if (preference === "normal") expect(value).toMatch(/^linear-gradient\(/);
      else expect(value, `${preference}: ${property}`).toBe("none");
    }
  } finally { sheet.remove(); }
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

test("text reflow inside an unchanged viewport refreshes its edges, and late resizes after close cannot update them", () => {
  const callbacks: (() => void)[] = [], observed: Element[] = [];
  vi.stubGlobal("ResizeObserver", class {
    constructor(callback: () => void) { callbacks.push(callback); }
    observe(node: Element) { observed.push(node); }
    disconnect() {}
  });
  const view = render(<Box />), box = view.getByTestId("box");
  let contentHeight = 100;
  Object.defineProperty(box, "clientHeight", { value: 100 });
  Object.defineProperty(box, "scrollHeight", { get: () => contentHeight });
  expect(observed).toContain(box.firstElementChild);
  contentHeight = 300;
  act(() => callbacks[0]!());
  expect(box.hasAttribute("data-more-below")).toBe(true);
  view.unmount();
  box.removeAttribute("data-more-below");
  act(() => callbacks[0]!());
  expect(box.hasAttribute("data-more-below")).toBe(false);
});
