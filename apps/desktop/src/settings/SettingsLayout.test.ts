import { expect, test, vi } from "vitest";

// Match the existing CSS-rule tests without adding a Node-types dependency.
const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const read = (path: string) => readFileSync(new FileURL(path, import.meta.url), "utf8");
const shell = read("../ui/SettingsLayout.css") + read("./Settings.css");
const groups = read("../ui/SettingsGroup.css");
const buttons = read("../ui/Button.css");
const switches = read("../ui/Switch.css");
const segments = read("../ui/Segmented.css");
const navigation = read("../ui/SideNavigation.css");
const tokens = read("../tokens.css");

function rules(source: string): CSSStyleRule[] {
  const sheet = document.createElement("style");
  sheet.textContent = source;
  document.head.append(sheet);
  try {
    return [...sheet.sheet!.cssRules].filter((rule) => rule.type === CSSRule.STYLE_RULE) as CSSStyleRule[];
  } finally { sheet.remove(); }
}

function style(source: string, selector: string): CSSStyleDeclaration {
  const rule = rules(source).find((rule) => rule.selectorText === selector);
  expect(rule, selector).toBeTruthy();
  return rule!.style;
}

test("Settings fills the viewport, with bounded page and navigation scrollers below a reserved native title bar", () => {
  expect(read("../../settings.html")).toMatch(/<html[^>]+class="settings-window"/);
  const roots = style(shell, ":root.settings-window, :root.settings-window body, #settings");
  expect(roots.getPropertyValue("height")).toBe("100%");
  expect(roots.getPropertyValue("overflow")).toBe("hidden");
  expect(roots.getPropertyValue("margin")).toBe("0px");
  const frame = style(shell, ".settings");
  // Percentage heights keep the whole root → body → mount → shell chain in
  // the same coordinate space when browser zoom or text scaling changes it.
  expect(frame.getPropertyValue("height")).toBe("100%");
  expect(frame.getPropertyValue("display")).toBe("grid");
  expect(frame.getPropertyValue("grid-template-columns")).toBe("var(--sidebar-width) minmax(0, 1fr)");
  expect(frame.getPropertyValue("grid-template-rows")).toBe("var(--titlebar-height) minmax(0, 1fr)");
  expect(frame.getPropertyValue("overflow")).toBe("hidden");
  expect(style(shell, ".settings-sidebar").getPropertyValue("grid-area")).toBe("1 / 1 / -1 / 2");
  expect(style(shell, ".settings-titlebar").getPropertyValue("grid-area")).toBe("1 / 1 / 2 / -1");
  const page = style(shell, ".settings-page");
  expect(page.getPropertyValue("grid-area")).toBe("1 / 2 / -1 / 3");
  for (const property of ["min-width", "min-height"]) expect(page.getPropertyValue(property)).toBe("0px");
  expect(page.getPropertyValue("overflow-x")).toBe("hidden");
  expect(page.getPropertyValue("overflow-y")).toBe("auto");
  expect(page.getPropertyValue("grid-auto-rows")).toBe("max-content");
  expect(page.getPropertyValue("align-content")).toBe("start");
  const scrollers = rules(shell + groups + navigation).filter((rule) => ["overflow", "overflow-y"].some((property) => /^(auto|scroll)$/.test(rule.style.getPropertyValue(property))));
  expect(scrollers.map((rule) => rule.selectorText)).toEqual([".settings-page", ".ui-side-navigation"]);
  for (const selector of [".settings", ".settings-titlebar", ".settings-page", ".settings-page-title"]) {
    const layout = style(shell, selector);
    expect(["sticky", "absolute", "fixed"]).not.toContain(layout.getPropertyValue("position"));
    for (const property of ["margin-top", "margin-bottom", "margin-block"]) expect(layout.getPropertyValue(property)).not.toMatch(/-|calc\(/);
  }
});

test.each([
  ["page", shell, ".settings-page"],
  ["sidebar", navigation, ".ui-side-navigation"],
])("the %s scroller contains absolutely positioned controls instead of extending the document", (_name, source, selector) => {
  expect(style(source, selector).getPropertyValue("position")).toBe("relative");
  expect(style(source, selector).getPropertyValue("overflow-y")).toBe("auto");
});

test("long sidebar labels can scroll within the remaining height without moving the title bar", () => {
  const sidebar = style(shell, ".settings-sidebar");
  expect(sidebar.getPropertyValue("display")).toBe("flex");
  expect(sidebar.getPropertyValue("flex-direction")).toBe("column");
  expect(sidebar.getPropertyValue("min-height")).toBe("0px");
  expect(sidebar.getPropertyValue("overflow")).toBe("hidden");
  expect(sidebar.getPropertyValue("padding-block-start")).toBe("var(--titlebar-height)");
  const list = style(navigation, ".ui-side-navigation");
  expect(list.getPropertyValue("flex-grow")).toBe("1");
  expect(list.getPropertyValue("min-height")).toBe("0px");
  expect(list.getPropertyValue("overflow-y")).toBe("auto");
  expect(list.getPropertyValue("box-sizing")).toBe("border-box");
  expect(list.getPropertyValue("overscroll-behavior")).toBe("contain");
  const item = style(navigation, ".ui-side-navigation-item");
  expect(item.getPropertyValue("flex-shrink")).toBe("0");
  expect(item.getPropertyValue("height")).toBe("");
  expect(item.getPropertyValue("overflow-wrap")).toBe("anywhere");
});

test("sidebar navigation and the large page title have shared selection, spacing and type roles", () => {
  const selected = style(navigation, '.ui-side-navigation-item[aria-current="page"]');
  expect(selected.getPropertyValue("background")).toBe("var(--selected)");
  expect(selected.getPropertyValue("color")).toBe("var(--selected-text)");
  const title = style(shell, ".settings-page-title");
  expect(title.getPropertyValue("font-size")).toBe("var(--type-display-size)");
  expect(title.getPropertyValue("line-height")).toBe("var(--type-display-line)");
  expect(style(shell, ".settings-page").getPropertyValue("padding")).toBe("0 var(--space-6) var(--space-6)");
  expect(tokens).toMatch(/--sidebar-width:\s*min\(12\.5rem, 26%\)/);
});

test("grouped rows and shared controls grow and wrap at the minimum window size and larger text", () => {
  const row = style(groups, ".ui-settings-row");
  expect(row.getPropertyValue("min-height")).toBe("var(--settings-row-h)");
  expect(row.getPropertyValue("height")).toBe("");
  expect(row.getPropertyValue("box-sizing")).toBe("border-box");
  expect(row.getPropertyValue("flex-wrap")).toBe("wrap");
  for (const selector of [".ui-settings-group", ".ui-settings-card"]) expect(style(groups, selector).getPropertyValue("flex-shrink")).toBe("0");
  expect(style(groups, ".ui-settings-card").getPropertyValue("border-radius")).toBe("var(--r-card)");
  const button = style(buttons, ".ui-button[data-variant]");
  expect(button.getPropertyValue("font")).toBe("inherit");
  expect(button.getPropertyValue("max-width")).toBe("100%");
  expect(button.getPropertyValue("overflow-wrap")).toBe("anywhere");
  expect(button.getPropertyValue("white-space")).toBe("normal");
  expect(style(switches, ".ui-switch").getPropertyValue("flex-shrink")).toBe("0");
  expect(style(segments, ".ui-segmented").getPropertyValue("flex-wrap")).toBe("wrap");
  expect(style(segments, ".ui-segment").getPropertyValue("white-space")).toBe("normal");
});

test("permission instructions and actions share the card's leading edge at every width", () => {
  const guide = style(shell, ".paste-guide");
  expect(guide.getPropertyValue("grid-template-columns")).toBe("minmax(0, 1fr)");
  expect(guide.getPropertyValue("text-align")).toBe("start");
  expect(guide.getPropertyValue("padding")).toBe("var(--space-4) var(--row-pad)");
  expect(style(shell, ".paste-guide-actions").getPropertyValue("justify-content")).toBe("start");
  expect(style(shell, ".paste-guide-text").getPropertyValue("min-width")).toBe("0px");
  expect(style(shell, ".paste-guide ol").getPropertyValue("padding-inline-start")).toBe("var(--space-5)");
});

test("picture choices use a quiet selected surface and an inward keyboard-only ring", () => {
  const choices = read("../ui/PictureChoice.css");
  expect(style(choices, ".ui-picture-choice[data-selected]").getPropertyValue("color")).toBe("var(--text)");
  expect(style(choices, ".ui-picture-choice[data-focus-visible]").getPropertyValue("outline-offset")).toBe("var(--ring-offset)");
  const sheet = document.createElement("style");
  sheet.textContent = choices;
  const picture = document.createElement("div");
  picture.className = "ui-picture-choice";
  picture.dataset.selected = "true";
  document.head.append(sheet); document.body.append(picture);
  try {
    expect(getComputedStyle(picture).outline).toBe("none");
    picture.dataset.focusVisible = "true";
    expect(getComputedStyle(picture).outline).toBe("var(--ring-width) solid var(--ring)");
    expect(getComputedStyle(picture).outlineOffset).toBe("var(--ring-offset)");
  } finally { picture.remove(); sheet.remove(); }
  expect(style(groups, ".ui-picker-picture").getPropertyValue("max-width")).toBe("100%");
  const media = (source: string) => {
    const sheet = document.createElement("style"); sheet.textContent = source; document.head.append(sheet);
    const rule = [...sheet.sheet!.cssRules].find((rule) => rule.type === CSSRule.MEDIA_RULE && (rule as CSSMediaRule).conditionText === "(pointer: coarse)") as CSSMediaRule;
    sheet.remove(); return [...rule.cssRules] as CSSStyleRule[];
  };
  expect(media(choices).find((rule) => rule.selectorText === ".ui-picture-choice")?.style.getPropertyValue("min-height")).toBe("var(--touch-target)");
  expect(media(groups).find((rule) => rule.selectorText === ".ui-settings-search input")?.style.getPropertyValue("min-height")).toBe("var(--touch-target)");
  expect(media(read("../ui/ToolbarMenu.css")).find((rule) => rule.selectorText === ".toolbar-menu-filter input")?.style.getPropertyValue("min-height")).toBe("var(--touch-target)");
});


