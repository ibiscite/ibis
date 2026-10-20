import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { App, type PickerApi } from "./App";
import { galleryHref, renderGalleryState } from "./gallery";
import { libraryMessages } from "./messages";
import { platformOf } from "./nativeAppearance";

const syntheticApi: PickerApi = {
  libraryStatus: async () => ({ state: "loaded", has_snapshot: true, zotero: false, zotero_libraries: [], name: "gallery.bib", snapshot_name: "gallery.bib", references: 1, warnings: 0, error: null, note: null, settings_notice: null }),
  search: async () => [{ id: { source: "synthetic-gallery", native_key: "example" }, key: "example", title: "Synthetic gallery reference", authors: "Example Author", year: 2026, matched: ["title"] }],
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

test("render acknowledgment waits for the synthetic search and its actual result rows", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", (id: number) => window.clearTimeout(id));
  const state = galleryStates.find((candidate) => candidate.id === "many-results")!;
  const provider = galleryProvider(state);
  let resolveSearch: (found: typeof state.results) => void = () => undefined;
  provider.api.search = () => new Promise((resolve) => { resolveSearch = (found) => resolve(found ?? []); });
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

test.each(galleryStates.map((state) => state.id))("the rendered %s fixture acknowledges exactly that state", async (id) => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", (frame: number) => window.clearTimeout(frame));
  const state = galleryStates.find((candidate) => candidate.id === id)!;
  const acknowledge = vi.fn(async () => undefined);
  const container = document.createElement("div");
  document.body.append(container);
  let stop: () => void = () => undefined;
  try {
    await act(async () => { stop = renderGalleryState(container, state, acknowledge); });
    await act(() => vi.advanceTimersByTimeAsync(1000));
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(acknowledge).toHaveBeenCalledExactlyOnceWith(id);
  } finally {
    await act(async () => stop());
    container.remove();
  }
});


test("gallery readiness follows the shared library summary used by the picker", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", (frame: number) => window.clearTimeout(frame));
  const messages = libraryMessages as typeof libraryMessages & { summary: (status: unknown) => string };
  const summary = vi.spyOn(messages, "summary").mockReturnValue("Synthetic translated library summary");
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
  ["menu-open", (root: HTMLElement) => root.querySelector('[role="menu"]') !== null],
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
