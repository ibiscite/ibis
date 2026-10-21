import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { App, type PickerApi } from "./App";
import { galleryHref, galleryThemeFor, renderGalleryState } from "./gallery";
import { libraryMessages } from "./messages";
import { platformOf } from "./nativeAppearance";

const syntheticApi: PickerApi = {
  libraryStatus: async () => ({ revision: "gallery", state: "loaded", has_snapshot: true, zotero: false, zotero_libraries: [], name: "gallery.bib", snapshot_name: "gallery.bib", references: 1, source_counts: { "synthetic-gallery": 1 }, warnings: 0, error: null, note: null, settings_notice: null, snapshot_zotero: false, stale: false, updated_at: null, checked_at: null }),
  search: async () => ({ revision: "gallery", results: [{ revision: "gallery", id: { source: "synthetic-gallery", native_key: "example" }, key: "example", title: "Synthetic gallery reference", authors: "Example Author", year: 2026, matched: ["title"], first_author: "Example Author", author_count: 1, container: null, item_type: "book" }] }),
  copyCitation: async () => ({ text: "[@example]", warnings: [] }),
  copyFormat: async () => "pandoc",
  pickerPreferences: async () => ({ citation_style: null, density: "comfortable", larger_text: false }),
  setPickerPreferences: async () => null,
  shortcutStatus: async () => ({ shortcut: "Ctrl+Shift+Y", registered: true, error: null }),
  chooseLibrary: async () => undefined,
  useZotero: async () => undefined,
  reloadLibrary: async () => undefined,
  hidePicker: async () => undefined,
  setPinned: async () => undefined,
  quit: async () => undefined,
  onLibraryChanged: async () => () => undefined,
  onPickerShown: async () => () => undefined,
  onPickerHidden: async () => () => undefined,
};

afterEach(async () => {
  vi.useRealTimers();
  cleanup();
  await Promise.resolve();
  clearMocks();
  vi.unstubAllGlobals();
  delete document.documentElement.dataset.galleryTheme;
  delete document.documentElement.dataset.gallerySize;
  delete document.documentElement.dataset.galleryRendered;
  document.documentElement.removeAttribute("dir");
});

test("a gallery picker uses synthetic providers for search and copy without native IPC", async () => {
  const nativeCalls: string[] = [];
  mockIPC((command) => { nativeCalls.push(command); throw new Error("Native commands must not run in the gallery"); }, { shouldMockEvents: true });
  render(<App api={syntheticApi} preview={{ query: "synthetic" }} />);
  await screen.findByText("Synthetic gallery reference");
  expect((screen.getByLabelText("Search references") as HTMLInputElement).value).toBe("synthetic");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByText("[@example]");
  expect(nativeCalls).toEqual([]);
});

test("the gallery keeps a seeded copy confirmation available for evidence capture", async () => {
  vi.useFakeTimers();
  render(<App api={syntheticApi} preview={{ copied: { text: "[@preview]", warnings: [] } }} />);
  await act(async () => { await Promise.resolve(); });
  expect(screen.getByText("[@preview]")).toBeTruthy();
  await act(() => vi.advanceTimersByTimeAsync(10_000));
  expect(screen.getByText("[@preview]")).toBeTruthy();
});

const { applyPlatformSwitch, galleryProvider, galleryStates, gallerySwitches, withSwitches } = await import("./gallery-fixtures");

test.each([
  ["first-run", "Choose a library to get started"],
  ["loading", "Opening example.bib…"],
  ["empty", "No references found in empty.bib. Choose a BibTeX or BibLaTeX export with references, then try again."],
  ["no-results", "No matching references"],
  ["invalid-settings", "settings.json could not be read. Ibis is using the backup. Changes are not being saved. Repair the synthetic settings file and restart Ibis."],
  ["damaged-library", "Some references could not be read. Export the library again to restore them."],
])("the %s fixture renders the actual picker without reading native data", async (id, message) => {
  const nativeCalls: string[] = [];
  mockIPC((command) => { nativeCalls.push(command); throw new Error("Native commands must not run in the gallery"); }, { shouldMockEvents: true });
  const state = galleryStates.find((candidate) => candidate.id === id);
  expect(state).toBeTruthy();
  render(<App {...galleryProvider(state!)} />);
  await screen.findByText(message);
  expect(nativeCalls).toEqual([]);
});

test("the many-results fixture exercises a scrollable real result list", async () => {
  const state = galleryStates.find((candidate) => candidate.id === "many-results");
  render(<App {...galleryProvider(state!)} />);
  await screen.findByText("Memory study 20: a synthetic reference");
  expect(within(screen.getByRole("listbox", { name: "Results" })).getAllByRole("option")).toHaveLength(20);
});

test("a gallery state with horizontal overflow is not accepted as rendered", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", (frame: number) => window.clearTimeout(frame));
  const state = galleryStates.find((candidate) => candidate.id === "large-text")!;
  const acknowledge = vi.fn(async () => undefined);
  const container = document.createElement("div");
  document.body.append(container);
  Object.defineProperties(container, {
    clientWidth: { value: 360 },
    scrollWidth: { value: 366, configurable: true },
  });
  let stop: () => void = () => undefined;
  try {
    await act(async () => { stop = renderGalleryState(container, state, acknowledge); });
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(acknowledge).not.toHaveBeenCalled();
    Object.defineProperty(container, "scrollWidth", { value: 360 });
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(acknowledge).toHaveBeenCalledExactlyOnceWith("large-text");
  } finally {
    await act(async () => stop());
    container.remove();
  }
});

test.each(["overflow", "document-overflow", "clipped", "covered"])("selected-reference evidence rejects %s content", async (fault) => {
  vi.useFakeTimers();
  vi.stubGlobal("innerHeight", 320);
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", (frame: number) => window.clearTimeout(frame));
  let overflow = true;
  const height = vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.tagName === "MAIN" ? (overflow && fault === "overflow" ? 700 : 320)
      : this.tagName === "HTML" && overflow && fault === "document-overflow" ? 700 : 0;
  });
  const client = vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(320);
  const geometry = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.tagName === "MAIN") return new DOMRect(0, 0, 720, 320);
    if (this.tagName === "FOOTER") return new DOMRect(0, 280, 720, 40);
    if (this.id === "search") return new DOMRect(40, 10, 400, 40);
    if (this.matches('footer button')) return new DOMRect(620, 284, 84, 32);
    if (this.matches(".results-area, .picker-content")) return new DOMRect(0, 80, 720, 200);
    if (this.hasAttribute("data-active")) return new DOMRect(0, overflow && fault === "clipped" ? 500 : 100, 720, 60);
    if (this.matches('[aria-label="Selected references"]')) return new DOMRect(0, 0, 720, 80);
    return new DOMRect(0, 0, 720, 80);
  });
  const state = galleryStates.find((candidate) => candidate.id === "tray")!;
  const acknowledge = vi.fn(async () => undefined);
  const container = document.createElement("div");
  document.body.append(container);
  let stop: () => void = () => undefined;
  const originalHit = document.elementFromPoint;
  document.elementFromPoint = (_x, y) => container.querySelector(y < 80 ? "#search" : y >= 280 ? 'footer button[aria-haspopup]'
    : overflow && fault === "covered" ? "footer" : "[data-active]");
  try {
    await act(async () => { stop = renderGalleryState(container, state, acknowledge); });
    await act(() => vi.advanceTimersByTimeAsync(1000));
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(acknowledge).not.toHaveBeenCalled();
    overflow = false;
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(acknowledge).toHaveBeenCalledExactlyOnceWith("tray");
  } finally {
    await act(async () => stop());
    container.remove();
    height.mockRestore(); client.mockRestore(); geometry.mockRestore();
    document.elementFromPoint = originalHit;
  }
});

test("render acknowledgment waits for the synthetic search and its actual result rows", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", (id: number) => window.clearTimeout(id));
  const state = galleryStates.find((candidate) => candidate.id === "many-results")!;
  const provider = galleryProvider(state);
  let resolveSearch: (found: typeof state.results) => void = () => undefined;
  provider.api.search = () => new Promise((resolve) => { resolveSearch = (found) => resolve({ revision: state.library.revision, results: found ?? [] }); });
  const acknowledge = vi.fn(async () => undefined);
  const container = document.createElement("div");
  document.body.append(container);
  let stop: () => void = () => undefined;
  try {
    await act(async () => { stop = renderGalleryState(container, state, acknowledge, provider); });
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(acknowledge).not.toHaveBeenCalled();
    await act(async () => resolveSearch(state.results));
    await act(() => vi.advanceTimersByTimeAsync(1000));
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(container.querySelectorAll('[role="option"]')).toHaveLength(20);
    expect(acknowledge).toHaveBeenCalledExactlyOnceWith("many-results");
  } finally {
    await act(async () => stop());
    container.remove();
  }
});

test("gallery diagnostics describe fixed render stages without acknowledging an unfinished App", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", (id: number) => window.clearTimeout(id));
  const state = galleryStates.find((candidate) => candidate.id === "first-run")!;
  const provider = galleryProvider(state);
  let resolveFormat: () => void = () => undefined;
  provider.api = { ...provider.api, copyFormat: () => new Promise((resolve) => { resolveFormat = () => resolve("pandoc"); }) };
  const acknowledge = vi.fn(async () => undefined);
  const stages: string[] = [];
  const container = document.createElement("div");
  document.body.append(container);
  let stop: () => void = () => undefined;
  try {
    await act(async () => { stop = renderGalleryState(container, state, acknowledge, provider, (stage) => stages.push(stage)); });
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(stages).toEqual(["frontend-mounted"]);
    expect(acknowledge).not.toHaveBeenCalled();
    await act(async () => resolveFormat());
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(stages).toEqual(["frontend-mounted", "effects-settled", "rendered"]);
    expect(acknowledge).toHaveBeenCalledExactlyOnceWith("first-run");
  } finally {
    await act(async () => stop());
    container.remove();
  }
});

test("render acknowledgment waits for startup effects even after the first-run UI appears", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", (id: number) => window.clearTimeout(id));
  const state = galleryStates.find((candidate) => candidate.id === "first-run")!;
  const provider = galleryProvider(state);
  let resolveFormat: () => void = () => undefined;
  provider.api = { ...provider.api, copyFormat: () => new Promise((resolve) => { resolveFormat = () => resolve("pandoc"); }) };
  const acknowledge = vi.fn(async () => undefined);
  const container = document.createElement("div");
  document.body.append(container);
  let stop: () => void = () => undefined;
  try {
    await act(async () => { stop = renderGalleryState(container, state, acknowledge, provider); });
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(container.textContent).toContain("Choose a library to get started");
    expect(acknowledge).not.toHaveBeenCalled();
    await act(async () => resolveFormat());
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(acknowledge).toHaveBeenCalledExactlyOnceWith("first-run");
  } finally {
    await act(async () => stop());
    container.remove();
  }
});

async function expectRenderedFixture(id: string): Promise<void> {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", (frame: number) => window.clearTimeout(frame));
  const state = galleryStates.find((candidate) => candidate.id === id)!;
  // This tests acknowledgement, not CSL throughput across 50 fake-frame changes.
  // Browser pressure captures use the real worker and formatter.
  const text = "Synthetic formatted preview pressure. ".repeat(50);
  const formatting = state.formatted ? vi.spyOn(await import("./format/formatted"), "formatCitation")
    .mockResolvedValue({ text, html: text, markup: [text], warnings: [] }) : null;
  const acknowledge = vi.fn(async () => undefined);
  const container = document.createElement("div");
  document.body.append(container);
  let stop: () => void = () => undefined;
  try {
    await act(async () => { stop = renderGalleryState(container, state, acknowledge); });
    if ((state.tray ?? 0) >= 20) {
      // Include the initial results announcement, then commit every interaction
      // separately. Bulk clock advances can seed against an uncommitted tree.
      for (let i = 0; i < state.tray! * 2 + 32 && acknowledge.mock.calls.length === 0; i++) {
        await act(() => vi.advanceTimersByTimeAsync(16));
      }
      expect(container.querySelectorAll('[aria-label="Selected references"] > li')).toHaveLength(state.tray!);
      expect(container.querySelector("[data-active]")?.id).toBe(`result-${state.tray! - 1}`);
      if (state.formatted) expect(container.querySelector(".format-preview .preview-text")?.textContent).toBe(text);
    } else {
      await act(() => vi.advanceTimersByTimeAsync(1000));
      await act(() => vi.advanceTimersByTimeAsync(1000));
    }
    expect(acknowledge).toHaveBeenCalledExactlyOnceWith(id);
  } finally {
    await act(async () => stop());
    container.remove();
    formatting?.mockRestore();
  }
}

test.each(galleryStates.filter((state) => (state.tray ?? 0) < 20).map((state) => state.id))(
  "the rendered %s fixture acknowledges exactly that state", expectRenderedFixture,
);
// Up to 100 committed interactions and full 50-row DOM checks exceed the default
// wall-clock limit on CI. Keep these bounded separately from app latency budgets.
test.each(galleryStates.filter((state) => (state.tray ?? 0) >= 20).map((state) => state.id))(
  "the rendered %s fixture acknowledges exactly that state", { timeout: 15_000 }, expectRenderedFixture,
);

test("gallery readiness follows the shared library summary used by the picker", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", (frame: number) => window.clearTimeout(frame));
  const summary = vi.spyOn(libraryMessages, "summaryParts").mockReturnValue([null, "Synthetic translated library summary"]);
  const state = galleryStates.find((candidate) => candidate.id === "damaged-library")!;
  const acknowledge = vi.fn(async () => undefined);
  const container = document.createElement("div");
  document.body.append(container);
  let stop: () => void = () => undefined;
  try {
    await act(async () => { stop = renderGalleryState(container, state, acknowledge); });
    await act(() => vi.advanceTimersByTimeAsync(1000));
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(container.querySelector('[aria-label="Library status"]')?.textContent).toBe("Synthetic translated library summary");
    expect(acknowledge).toHaveBeenCalledExactlyOnceWith("damaged-library");
  } finally {
    await act(async () => stop());
    summary.mockRestore();
    container.remove();
  }
});

test("review switches accept only known values, once each, and never in the native evidence app", () => {
  expect(gallerySwitches("?platform=windows&theme=dark&text=large&density=compact", false))
    .toEqual({ platform: "windows", theme: "dark", text: "large", density: "compact" });
  expect(gallerySwitches("?platform=macos&platform=linux&theme=sepia&text=LARGE&density=", false)).toEqual({});
  expect(gallerySwitches("?platform=%3Cscript%3E&state=pinned", false)).toEqual({});
  expect(gallerySwitches("?platform=windows&theme=dark", true)).toEqual({});
});

test("a state's own theme, density and text setting win over the switches", () => {
  const switches = { theme: "dark", text: "large", density: "spacious" } as const;
  const plain = withSwitches(galleryStates.find((state) => state.id === "pinned")!, switches);
  expect([plain.theme, plain.density, plain.largerTextSetting]).toEqual(["dark", "spacious", true]);
  const own = withSwitches(galleryStates.find((state) => state.id === "compact")!, switches);
  expect(own.density).toBe("compact");
  expect(withSwitches(galleryStates.find((state) => state.id === "high-contrast")!, switches).theme).toBe("contrast");
  expect(withSwitches(galleryStates.find((state) => state.id === "pinned")!, {}).largerTextSetting).toBeUndefined();
});

test("the high-contrast state uses the real contrast rules when the browser applies them", () => {
  expect(galleryThemeFor("contrast", false)).toBe("contrast");
  expect(galleryThemeFor("contrast", true)).toBeUndefined();
  expect(galleryThemeFor("dark", true)).toBe("dark");
  expect(galleryThemeFor(undefined, false)).toBe("light");
});

test.each(["macos", "windows", "linux"] as const)("the %s platform switch is what the picker reads as the platform", (platform) => {
  const target = {} as Navigator;
  applyPlatformSwitch(platform, target);
  expect(platformOf(target.platform)).toBe(platform);
});

test("gallery links keep the switches and drop unset ones", () => {
  expect(galleryHref({ platform: "linux", theme: "dark" }, { state: "pinned" })).toBe("./gallery.html?state=pinned&platform=linux&theme=dark");
  expect(galleryHref({ platform: "linux" }, { platform: "macos" })).toBe("./gallery.html?platform=macos");
  expect(galleryHref({})).toBe("./gallery.html");
});

async function renderUntilAcknowledged(id: string, provider = galleryProvider(galleryStates.find((state) => state.id === id)!),
  state = galleryStates.find((candidate) => candidate.id === id)!) {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", (frame: number) => window.clearTimeout(frame));
  const acknowledge = vi.fn(async () => undefined);
  const container = document.createElement("div");
  document.body.append(container);
  const stop = renderGalleryState(container, state, acknowledge, provider);
  for (let i = 0; i < 4; i++) await act(() => vi.advanceTimersByTimeAsync(1000));
  return { acknowledge, container, stop: async () => { await act(async () => stop()); container.remove(); } };
}

test.each([
  ["pinned", (root: HTMLElement) => root.querySelector('button[aria-label="Pin Ibis"]')?.getAttribute("aria-pressed") === "true"],
  ["menu-open", () => document.querySelector('[role="menu"]') !== null],
  ["keys", (root: HTMLElement) => root.querySelector('[role="dialog"]') !== null],
  ["tray", (root: HTMLElement) => [...root.querySelectorAll('ul[aria-label="Selected references"] code')].map((key) => key.textContent).join() === "example2026,研究2026"],
  ["compact", () => document.documentElement.dataset.density === "compact"],
  ["spacious", () => document.documentElement.dataset.density === "spacious"],
  ["larger-text", () => document.documentElement.dataset.textSize === "larger"],
])("the %s review state is on screen before it is acknowledged", async (id, shown) => {
  const run = await renderUntilAcknowledged(id);
  try {
    expect(shown(run.container)).toBe(true);
    expect(run.acknowledge).toHaveBeenCalledExactlyOnceWith(id);
  } finally {
    await run.stop();
  }
});

test("a seeded interaction that does not take is never acknowledged", async () => {
  const state = galleryStates.find((candidate) => candidate.id === "pinned")!;
  const provider = galleryProvider(state);
  provider.api = { ...provider.api, setPinned: async () => { throw new Error("synthetic pin failure"); } };
  const run = await renderUntilAcknowledged("pinned", provider);
  try {
    expect(run.container.querySelector('button[aria-label="Pin Ibis"]')?.getAttribute("aria-pressed")).toBe("false");
    expect(run.acknowledge).not.toHaveBeenCalled();
  } finally {
    await run.stop();
  }
});

test("switched settings reach the rendered picker", async () => {
  const state = withSwitches(galleryStates.find((candidate) => candidate.id === "many-results")!, { theme: "dark", text: "large", density: "compact" });
  const run = await renderUntilAcknowledged("many-results", galleryProvider(state), state);
  try {
    expect(document.documentElement.dataset.galleryTheme).toBe("dark");
    expect(document.documentElement.dataset.density).toBe("compact");
    expect(document.documentElement.dataset.textSize).toBe("larger");
    expect(run.acknowledge).toHaveBeenCalledExactlyOnceWith("many-results");
  } finally {
    await run.stop();
  }
});

test("outside the native app a rendered state is marked on the page for browser captures", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", (frame: number) => window.clearTimeout(frame));
  const state = galleryStates.find((candidate) => candidate.id === "tray")!;
  const container = document.createElement("div");
  document.body.append(container);
  let stop: () => void = () => undefined;
  try {
    await act(async () => { stop = renderGalleryState(container, state); });
    for (let i = 0; i < 4; i++) await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(document.documentElement.dataset.galleryRendered).toBe("tray");
  } finally {
    await act(async () => stop());
    container.remove();
  }
});
