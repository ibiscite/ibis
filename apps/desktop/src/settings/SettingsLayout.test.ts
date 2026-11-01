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
  expect(page.getPropertyValue("grid-area")).toBe("2 / 2");
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
  expect(selected.getPropertyValue("background")).toBe("var(--sel)");
  expect(selected.getPropertyValue("color")).toBe("var(--sel-text)");
  const title = style(shell, ".settings-page-title");
  expect(title.getPropertyValue("font-size")).toBe("var(--type-page-title-size)");
  expect(title.getPropertyValue("line-height")).toBe("var(--type-page-title-line)");
  expect(style(shell, ".settings-page").getPropertyValue("padding")).toBe("var(--space-6)");
  expect(tokens).toMatch(/--type-page-title-size:\s*1\.75rem/);
  expect(tokens).toMatch(/--sidebar-width:\s*min\(12rem, 26%\)/);
});

test("grouped rows and shared controls grow and wrap at the minimum window size and larger text", () => {
  const row = style(groups, ".ui-settings-row");
  expect(row.getPropertyValue("min-height")).toBe("3rem");
  expect(row.getPropertyValue("height")).toBe("");
  expect(row.getPropertyValue("box-sizing")).toBe("border-box");
  expect(row.getPropertyValue("flex-wrap")).toBe("wrap");
  expect(style(groups, ".ui-settings-row[data-described]").getPropertyValue("min-height")).toBe("3.5rem");
  for (const selector of [".ui-settings-group", ".ui-settings-card"]) expect(style(groups, selector).getPropertyValue("flex-shrink")).toBe("0");
  expect(style(groups, ".ui-settings-card").getPropertyValue("border-radius")).toBe("var(--r-win)");
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

test("the existing eight Settings accents keep white text at 4.5:1", () => {
  const expected = ["0064e1", "953d96", "c8317d", "c42b30", "b25300", "8a6a00", "3b7f25", "6e6e73"];
  const shades = [...tokens.matchAll(/--accent-[a-z]+:\s*#([0-9a-f]{6})/g)].map((match) => match[1]!);
  expect(shades).toEqual(expected);
  for (const shade of shades) {
    const linear = shade.match(/../g)!.map((channel) => {
      const value = Number.parseInt(channel, 16) / 255;
      return value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4;
    });
    const luminance = linear[0]! * .2126 + linear[1]! * .7152 + linear[2]! * .0722;
    expect(1.05 / (luminance + .05), shade).toBeGreaterThanOrEqual(4.5);
    expect(["c23a1c", "ff7a5c", "d2401f"]).not.toContain(shade);
  }
});

test("picture choices use a quiet selected surface and an inward keyboard-only ring", () => {
  const choices = read("../ui/PictureChoice.css");
  expect(style(choices, ".ui-picture-choice[data-selected]").getPropertyValue("background")).toBe("color-mix(in srgb, var(--accent) 12%, var(--surface))");
  expect(style(choices, ".ui-picture-choice[data-selected]").getPropertyValue("color")).toBe("var(--text)");
  const selectionRing = "inset 0 0 0 2px var(--accent)";
  expect(style(choices, ".ui-picture-choice[data-selected]").getPropertyValue("box-shadow")).toBe(selectionRing);
  expect(style(choices, ".ui-picture-choice[data-focus-visible]").getPropertyValue("outline-offset")).toBe("calc(-1 * (var(--ring-width) + var(--ring-offset)))");
  expect(style(choices, ".ui-picture-choice[data-selected][data-focus-visible]").getPropertyValue("outline-offset")).toBe("calc(-1 * (2px + var(--ring-width) + var(--ring-offset)))");
  const sheet = document.createElement("style");
  sheet.textContent = choices;
  const picture = document.createElement("div");
  picture.className = "ui-picture-choice";
  picture.dataset.selected = "true";
  document.head.append(sheet); document.body.append(picture);
  try {
    expect(getComputedStyle(picture).boxShadow).toBe(selectionRing);
    expect(getComputedStyle(picture).outline).toBe("none");
    picture.dataset.focusVisible = "true";
    expect(getComputedStyle(picture).boxShadow).toBe(selectionRing);
    expect(getComputedStyle(picture).outline).toBe("var(--ring-width) solid var(--ring)");
    expect(getComputedStyle(picture).outlineOffset).toBe("calc(-1 * (2px + var(--ring-width) + var(--ring-offset)))");
  } finally { picture.remove(); sheet.remove(); }
  expect(style(groups, ".ui-picker-picture").getPropertyValue("max-width")).toBe("100%");
  expect(style(groups, ".ui-picker-picture").getPropertyValue("aspect-ratio")).toBe("4 / 3");
  const media = (source: string) => {
    const sheet = document.createElement("style"); sheet.textContent = source; document.head.append(sheet);
    const rule = [...sheet.sheet!.cssRules].find((rule) => rule.type === CSSRule.MEDIA_RULE && (rule as CSSMediaRule).conditionText === "(pointer: coarse)") as CSSMediaRule;
    sheet.remove(); return [...rule.cssRules] as CSSStyleRule[];
  };
  expect(media(choices).find((rule) => rule.selectorText === ".ui-picture-choice")?.style.getPropertyValue("min-height")).toBe("var(--touch-target)");
  expect(media(groups).find((rule) => rule.selectorText === ".ui-settings-search input")?.style.getPropertyValue("min-height")).toBe("var(--touch-target)");
  expect(media(read("../ui/ToolbarMenu.css")).find((rule) => rule.selectorText === ".toolbar-menu-filter input")?.style.getPropertyValue("min-height")).toBe("var(--touch-target)");
});


test("brand scarlet keeps 3:1 control contrast and 4.5:1 text through both themes and button states", () => {
  const brand = JSON.parse(read("../../../../brand/settings.json")).colours;
  const shades = tokens.match(/--accent-ibis:\s*light-dark\((#[0-9a-f]{6}), (#[0-9a-f]{6})\)/i)!;
  const on = tokens.match(/--accent-ibis-on:\s*light-dark\((#[0-9a-f]{6}), (#[0-9a-f]{6})\)/i)!;
  expect(shades, "brand accent must use the brand's two real scarlets").toBeTruthy();
  expect(on, "brand accent must pair a foreground in each theme").toBeTruthy();
  expect(shades.slice(1).map((hex) => hex.toUpperCase())).toEqual([brand.scarlet, brand.scarlet_dark]);
  const rgb = (hex: string) => hex.slice(1).match(/../g)!.map((channel) => Number.parseInt(channel, 16));
  const luminance = (channels: number[]) => channels.map((channel) => {
    const n = channel / 255;
    return n <= .04045 ? n / 12.92 : ((n + .055) / 1.055) ** 2.4;
  }).reduce((sum, n, i) => sum + n * [.2126, .7152, .0722][i]!, 0);
  const contrast = (a: number[], b: number[]) => {
    const x = luminance(a), y = luminance(b);
    return (Math.max(x, y) + .05) / (Math.min(x, y) + .05);
  };
  for (const [theme, surfaces] of [[1, ["#ffffff", "#fafafc", "#f0f0f2"]], [2, ["#1e1e22", "#303034", "#26262b"]]] as const) {
    const fill = rgb(shades[theme]!), foreground = rgb(on[theme]!);
    for (const amount of [1, .92, .84]) {
      const state = fill.map((channel) => channel * amount);
      expect(contrast(state, foreground), `theme ${theme}, button state ${amount}`).toBeGreaterThanOrEqual(4.5);
      for (const surface of surfaces) expect(contrast(state, rgb(surface)), `theme ${theme}, surface ${surface}`).toBeGreaterThanOrEqual(3);
    }
  }
  // An on-switch is another meaningful foreground on the accent, so its
  // thumb must consume the same paired colour rather than unconditional white.
  expect(style(switches, ".ui-switch[data-selected] .ui-switch-thumb").getPropertyValue("background")).toBe("var(--on-accent)");
  expect(tokens).toContain('--accent-choice-on: var(--accent-ibis-on)');
  expect(tokens).toContain('--sel-text: var(--accent-choice-on); --on-accent: var(--accent-choice-on)');
});
