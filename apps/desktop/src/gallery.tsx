// First, so a ?platform= switch is in place as early as this page allows.
import { galleryProvider, galleryStates, gallerySwitches, withSwitches, type GallerySwitches, type GalleryState } from "./gallery-fixtures";
import { renderToolbarMenuGallery, toolbarMenuGalleryStates } from "./ui/ToolbarMenuGallery";
import { chipGroupGalleryStates, renderChipGroupGallery } from "./ui/ChipGroupGallery";
import { buttonGalleryStates, renderButtonGallery } from "./ui/ButtonGallery";
import { createRoot } from "react-dom/client";
import { isTauri } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { App, type PickerApi } from "./App";
import { SHELF_CHIPS } from "./features/picker/CitationShelf";
import { libraryMessages, pickerText, setupText } from "./messages";
import { hasRetainedFreshnessProblem } from "./LibraryStatusLine";
import { setPlatform } from "./nativeAppearance";
import "./styles.css";
import "./gallery.css";
import "./notice.css";
import { noticePlatform, renderNotice } from "./copyFeedback";

function rendered(root: HTMLElement, state: GalleryState): boolean {
  // Real webviews measure layout here; jsdom's zero dimensions do not prove it.
  // Keep screenshots with sideways overflow from being accepted as a good state.
  const fits = [document.documentElement, root, root.querySelector("main")]
    .every((element) => !element || element.scrollWidth <= element.clientWidth);
  const library = state.library;
  const input = root.querySelector<HTMLInputElement>("#search");
  const rows = [...root.querySelectorAll('#results [role="option"]')];
  const results = state.results ?? [];
  const label = libraryMessages.summary(library);
  const alerts = [library.settings_notice, library.error && !hasRetainedFreshnessProblem(library) ? libraryMessages.openError(library.name, library.error, library.zotero) : null]
    .filter((text): text is string => text !== null);
  const actualAlerts = [...root.querySelectorAll('[role="alert"]')].map((alert) => alert.textContent);
  const searched = library.state !== "none" && library.state !== "loading" && !(library.state === "error" && !library.has_snapshot);
  const announcement = searched ? `${results.length} ${results.length === 1 ? "result" : "results"}` : "";
  const busy = library.state === "loading" ? `Opening ${library.name ?? "library"}…`
    : library.state === "reloading" ? `Reloading ${library.name ?? "library"}…` : null;
  return fits && input?.value === (state.query ?? "")
    && root.querySelector('[aria-label="Library status"]')?.textContent === label
    && root.querySelector('[aria-label="Search results"]')?.textContent === announcement
    && rows.length === results.length
    && rows.every((row, index) => row.querySelector("strong")?.textContent === results[index]?.title
      && row.querySelector(".result-key")?.textContent === results[index]?.key)
    && JSON.stringify(actualAlerts) === JSON.stringify(alerts)
    && (root.querySelector('.empty-loading[role="progressbar"]')?.getAttribute("aria-label") ?? null) === busy
    && (root.querySelector('[aria-label="Choose library to get started"]') !== null) === (library.state === "none")
    && (state.copied ? root.querySelector(".copy-notice") !== null
      && (root.querySelector(".copy-notice code")?.textContent ?? root.querySelector(".shelf-output .preview-text")?.textContent) === state.copied.text
      : root.querySelector(".copy-notice") === null)
    && (root.querySelector('.library-bar p[role="status"]')?.textContent ?? null) === library.note
    && (document.querySelector(".gallery-notice .copied-notice code")?.textContent ?? null) === (state.notice ?? null);
}

const pinButton = (root: HTMLElement) => root.querySelector<HTMLButtonElement>('header .pin-button[aria-pressed="true"]');

function press(target: Element | null, key: string, init: KeyboardEventInit = {}): void {
  target?.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init }));
}

/** What a person would do to reach a state, one step per frame once the picker has rendered. */
function seedSteps(root: HTMLElement, state: GalleryState): (() => void)[] {
  const steps: (() => void)[] = [];
  const search = () => root.querySelector("#search");
  if (state.sources) steps.push(() => [...root.querySelectorAll<HTMLButtonElement>(".library-setup button")]
    .find((button) => button.textContent === setupText.findLibraries)?.click());
  for (let i = 0; i < (state.tray ?? 0); i++) {
    if (i > 0) steps.push(() => press(search(), "ArrowDown"));
    steps.push(() => press(search(), "Enter", { shiftKey: true }));
  }
  if (state.trayExpanded) steps.push(() => root.querySelector<HTMLButtonElement>('.chip-more:not(.chip-fit-measure) button')?.click());
  if (state.pinned) steps.push(() => press(search(), "P", { code: "KeyP", shiftKey: true, ...(navigator.platform.includes("Mac") ? { metaKey: true } : { ctrlKey: true }) }));
  if (state.keys) steps.push(() => press(root.querySelector("main"), "?"));
  if (state.menuOpen) steps.push(() => root.querySelector<HTMLButtonElement>('footer button[aria-haspopup]')?.click());
  if (state.formatMenu) steps.push(() => root.querySelector<HTMLButtonElement>(".toolbar-menu-button")?.click());
  const filter = state.formatMenu;
  if (typeof filter === "string") steps.push(() => {
    const input = document.querySelector<HTMLInputElement>(".toolbar-menu-filter input");
    if (!input) return;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, filter);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  return steps;
}

function selectionContained(root: HTMLElement, state: GalleryState): boolean {
  const main = root.querySelector("main");
  // Zero-size JSDOM geometry is not browser evidence.
  if (!state.tray || !main?.getBoundingClientRect().width) return true;
  const content = root.querySelector(".picker-content")?.getBoundingClientRect();
  const viewport = root.querySelector(".results-area")?.getBoundingClientRect();
  const active = root.querySelector<HTMLElement>("[data-active]");
  const row = active?.getBoundingClientRect();
  if (!content || !viewport || !active || !row) return false;
  const top = Math.max(0, content.top, viewport.top);
  const bottom = Math.min(innerHeight, content.bottom, viewport.bottom);
  const visibleTop = Math.max(top, row.top);
  const visibleBottom = Math.min(bottom, row.bottom);
  const hit = (node: Element, x: number, y: number) => typeof document.elementFromPoint !== "function"
    || node.contains(document.elementFromPoint(x, y));
  // The scrolling content, then the shelf, then the footer: what you browse
  // above what you get. Under pressure the results scroll inside the content.
  const bands = [root.querySelector(".picker-content"), root.querySelector(".shelf"), root.querySelector("footer")]
    .filter((node) => node !== null).map((node) => node.getBoundingClientRect());
  return main.scrollTop === 0 && main.scrollHeight <= main.clientHeight + 1 && document.documentElement.scrollHeight <= innerHeight + 1
    && bottom - top >= 24 && visibleBottom - visibleTop >= Math.min(row.height, bottom - top) - 1
    && hit(active, (Math.max(row.left, content.left) + Math.min(row.right, content.right)) / 2, (visibleTop + visibleBottom) / 2)
    && bands.every((box, i) => i === 0 || bands[i - 1]!.bottom <= box.top + 1)
    && [".search-bar", "footer"].every((selector) => {
      const box = root.querySelector(selector)?.getBoundingClientRect();
      return box && box.top >= 0 && box.bottom <= innerHeight + 1;
    })
    && [root.querySelector("#search"), root.querySelector('footer button[aria-haspopup]')].every((node) => {
      const box = node?.getBoundingClientRect();
      return node && box && box.top >= 0 && box.bottom <= innerHeight + 1 && hit(node, (box.left + box.right) / 2, (box.top + box.bottom) / 2);
    });
}

/** Expanded hit areas must stay inside the scrollport and belong to their button. */
function shelfTargetsContained(root: HTMLElement): boolean {
  const list = root.querySelector(".shelf-chips");
  const viewport = list?.getBoundingClientRect();
  if (!viewport?.width || !list?.clientWidth || typeof document.elementFromPoint !== "function") return true;
  const minimum = parseFloat(getComputedStyle(document.documentElement).fontSize) * (matchMedia("(pointer: coarse)").matches ? 2.75 : 2);
  let tested = 0;
  const contained = [...list.querySelectorAll<HTMLElement>(".chip:not([hidden]) button, .chip-more:not(.chip-fit-measure) button")].every((button) => {
    const box = button.getBoundingClientRect();
    // Partly scrolled rows are not the complete visible controls under test.
    if (box.top < viewport.top || box.bottom > viewport.bottom) return true;
    tested++;
    const hit = getComputedStyle(button, "::after");
    const left = box.left + (parseFloat(hit.left) || 0);
    const right = box.right - (parseFloat(hit.right) || 0);
    const top = box.top + (parseFloat(hit.top) || 0);
    const bottom = box.bottom - (parseFloat(hit.bottom) || 0);
    return right - left >= minimum - 0.1 && bottom - top >= minimum - 0.1
      && left >= viewport.left - 0.1 && right <= viewport.right + 0.1 && top >= viewport.top - 0.1 && bottom <= viewport.bottom + 0.1
      && [[left + 1, (top + bottom) / 2], [right - 1, (top + bottom) / 2], [(left + right) / 2, top + 1], [(left + right) / 2, bottom - 1]]
        .every(([x, y]) => button.contains(document.elementFromPoint(x!, y!)));
  });
  return tested > 0 && contained;
}

/** The complete warning action stays above its divider, including touch targets. */
function warningTargetContained(root: HTMLElement): boolean {
  const button = root.querySelector<HTMLElement>(".copy-notice .dismiss");
  const notice = button?.closest(".copy-notice")?.getBoundingClientRect();
  // Zero-size JSDOM geometry does not establish rendered containment.
  if (!button || !notice?.width) return true;
  const box = button.getBoundingClientRect();
  const minimum = (parseFloat(getComputedStyle(document.documentElement).fontSize) || 16)
    * (matchMedia("(pointer: coarse)").matches ? 2.75 : 2);
  return box.width >= minimum - 0.1 && box.height >= minimum - 0.1
    && box.left >= notice.left - 0.1 && box.right <= notice.right + 0.1
    && box.top >= notice.top - 0.1 && box.bottom <= notice.bottom + 0.1
    && (typeof document.elementFromPoint !== "function" || [[box.left + 1, box.y + box.height / 2],
      [box.right - 1, box.y + box.height / 2], [box.x + box.width / 2, box.top + 1], [box.x + box.width / 2, box.bottom - 1]]
      .every(([x, y]) => button.contains(document.elementFromPoint(x!, y!))));
}

/** The seeded settings and interactions are on screen, and nothing else is. */
function seeded(root: HTMLElement, state: GalleryState): boolean {
  const html = document.documentElement;
  // Menus open in a popover outside the picker's root.
  const menu = document.querySelector<HTMLElement>('[role="menu"]');
  const filter = document.querySelector<HTMLInputElement>(".toolbar-menu-filter input");
  const box = menu?.getBoundingClientRect();
  // Real browser captures require a safe viewport inset; zero-size jsdom is no proof.
  const menuFits = !box || box.width === 0 || (box.top >= 8 && box.left >= 8 && box.bottom <= innerHeight - 8 && box.right <= innerWidth - 8);
  // A menu still fading in is not its final look.
  return menuFits && !document.querySelector("[data-entering]") && html.dataset.density === (state.density ?? "comfortable")
    && html.dataset.textSize === (state.largerTextSetting ? "larger" : "normal")
    && (pinButton(root) !== null) === Boolean(state.pinned)
    && (document.getElementById("more-menu") !== null) === Boolean(state.menuOpen)
    && (document.querySelector(".toolbar-menu") !== null) === Boolean(state.formatMenu)
    && (typeof state.formatMenu !== "string" || filter?.value === state.formatMenu)
    && (root.querySelector('[role="dialog"]') !== null) === Boolean(state.keys)
    && root.querySelectorAll(`ul[aria-label="${pickerText.selectedReferences}"] > li.chip`).length === (state.trayExpanded ? state.tray : Math.min(state.tray ?? 0, SHELF_CHIPS))
    && Boolean(root.querySelector('.shelf-chips[data-expanded]')) === Boolean(state.trayExpanded)
    && (state.showPastePreview === false ? root.querySelector(".shelf-output") === null
      && root.querySelector('footer [aria-keyshortcuts="Enter"]') !== null
      : !(state.formatted || state.showPastePreview) || root.querySelector(".shelf-output .preview-text") !== null)
    && (!state.sources || root.querySelectorAll(".found-files li").length === state.sources.files.length)
    && selectionContained(root, state) && shelfTargetsContained(root) && warningTargetContained(root);
}

/**
 * The explicit contrast copy (data-gallery-theme="contrast") is for the native
 * evidence app, which cannot ask its webview for increased contrast. A browser
 * that already applies it, such as the contact sheet's Playwright emulation,
 * renders the real prefers-contrast and forced-colors rules instead.
 */
export function galleryThemeFor(theme: GalleryState["theme"], systemContrast = typeof matchMedia === "function"
  && matchMedia("(prefers-contrast: more), (forced-colors: active)").matches): "light" | "dark" | "contrast" | undefined {
  return theme === "contrast" && systemContrast ? undefined : theme ?? "light";
}

type GalleryDiagnosticStage = "frontend-mounted" | "effects-settled" | "rendered";

function reportGalleryStage(state: string, stage: GalleryDiagnosticStage): void {
  if (isTauri() && window.top === window) {
    void emitTo("main", "gallery:stage", { state, stage }).catch(() => console.error("Gallery diagnostic stage could not be sent."));
  }
}

/** Only the evidence entry can acknowledge its completed synthetic App render. */
export function renderGalleryState(
  root: HTMLElement,
  state: GalleryState,
  acknowledge: (state: string) => Promise<void> = async (id) => {
    if (isTauri()) {
      if (window.top === window) await emitTo("main", "gallery:rendered", id);
    } else {
      // Browser captures (scripts/gallery-contact-sheet.mjs) wait for this.
      document.documentElement.dataset.galleryRendered = id;
    }
  },
  provider = galleryProvider(state),
  reportStage: (stage: GalleryDiagnosticStage) => void = (stage) => reportGalleryStage(state.id, stage),
): () => void {
  let pending = 0;
  let failed = false;
  let fontsReady = false;
  let stopped = false;
  let mountedReported = false;
  let settledReported = false;
  let stableFrames = 0;
  let frame = 0;
  let step = 0;
  const steps = seedSteps(root, state);
  const track = <T,>(action: () => Promise<T>) => {
    pending++;
    return action().then((value) => { pending--; return value; }, (error: unknown) => {
      pending--; failed = true; throw error;
    });
  };
  const api: PickerApi = {
    ...provider.api,
    libraryStatus: () => track(() => provider.api.libraryStatus()),
    shortcutStatus: () => track(() => provider.api.shortcutStatus()),
    copyFormat: () => track(() => provider.api.copyFormat()),
    pickerPreferences: () => track(() => provider.api.pickerPreferences()),
    setPinned: (on) => track(() => provider.api.setPinned(on)),
    search: (query, limit) => track(() => provider.api.search(query, limit)),
    findSources: provider.api.findSources ? () => track(() => provider.api.findSources!()) : undefined,
    onLibraryChanged: (handler) => track(() => provider.api.onLibraryChanged(handler)),
    onPickerShown: (handler) => track(() => provider.api.onPickerShown(handler)),
  };
  const theme = galleryThemeFor(state.theme);
  if (theme) document.documentElement.dataset.galleryTheme = theme;
  else delete document.documentElement.dataset.galleryTheme;
  document.documentElement.dataset.gallerySize = state.largeText ? "large" : "normal";
  document.documentElement.dir = state.direction ?? "ltr";
  document.title = `${state.label} — Ibis picker states`;
  const app = createRoot(root);
  app.render(<App api={api} preview={provider.preview} />);
  // The notice is its own window in the app; here it is drawn where it appears.
  let notice: HTMLElement | null = null;
  if (state.notice !== undefined) {
    notice = document.createElement("div");
    notice.className = "gallery-notice";
    notice.dataset.platform = noticePlatform(navigator.platform);
    document.body.append(notice);
    renderNotice(notice, noticePlatform(navigator.platform))(state.notice);
  }
  void (document.fonts?.ready ?? Promise.resolve()).then(() => { fontsReady = true; }, () => { failed = true; });
  const check = () => {
    if (stopped || !root.isConnected) return;
    if (!mountedReported && root.querySelector("#search")) {
      reportStage("frontend-mounted");
      mountedReported = true;
    }
    if (!settledReported && mountedReported && !failed && fontsReady && pending === 0) {
      reportStage("effects-settled");
      settledReported = true;
    }
    const ready = !failed && fontsReady && document.fonts?.status !== "loading" && pending === 0 && rendered(root, state);
    if (ready && step < steps.length) {
      steps[step++]!();
      stableFrames = 0;
    } else {
      stableFrames = ready && step === steps.length && seeded(root, state) ? stableFrames + 1 : 0;
    }
    if (stableFrames >= 2) {
      reportStage("rendered");
      void acknowledge(state.id).catch(() => console.error("Gallery render acknowledgment failed."));
      return;
    }
    frame = requestAnimationFrame(check);
  };
  frame = requestAnimationFrame(check);
  return () => { stopped = true; cancelAnimationFrame(frame); app.unmount(); notice?.remove(); };
}

const SWITCH_CHOICES: { name: keyof GallerySwitches; label: string; values: [string, string][] }[] = [
  { name: "platform", label: "Look", values: [["macos", "macOS"], ["windows", "Windows"], ["linux", "Linux"]] },
  { name: "theme", label: "Theme", values: [["light", "Light"], ["dark", "Dark"]] },
  { name: "text", label: "Text", values: [["normal", "Normal"], ["large", "Larger"]] },
  { name: "density", label: "Rows", values: [["compact", "Compact"], ["comfortable", "Comfortable"], ["spacious", "Spacious"]] },
];

/** A gallery link that keeps the current switches. */
export function galleryHref(switches: GallerySwitches, changes: Partial<Record<keyof GallerySwitches | "state", string>> = {}): string {
  const params = new URLSearchParams();
  const all: Record<string, string | undefined> = { ...switches, ...changes };
  for (const name of ["state", "platform", "theme", "text", "density"]) {
    const value = all[name];
    if (value) params.set(name, value);
  }
  const query = params.toString();
  return `./gallery.html${query ? `?${query}` : ""}`;
}

/** Separate evidence/dev entry: the normal production build omits this page. */
function Gallery({ switches }: { switches: GallerySwitches }) {
  return (
    <main className="gallery-catalog">
      <header>
        <h1>Picker states</h1>
        <p>Synthetic references for reviewing the picker. Open a state on its own to inspect it at full size.</p>
        {!isTauri() && (
          <nav aria-label="Review switches">
            {SWITCH_CHOICES.map(({ name, label, values }) => (
              <p key={name}>
                {label}:{" "}
                {values.map(([value, text], i) => (
                  <span key={value}>
                    {i > 0 && " · "}
                    <a href={galleryHref(switches, { [name]: value })} aria-current={switches[name] === value || undefined}>{text}</a>
                  </span>
                ))}
              </p>
            ))}
          </nav>
        )}
      </header>
      <div className="gallery-grid">
        {[...galleryStates, ...(isTauri() ? [] : [...toolbarMenuGalleryStates, ...chipGroupGalleryStates, ...buttonGalleryStates])].map((state) => (
          <section key={state.id} aria-label={state.label}>
            <h2><a href={galleryHref(switches, { state: state.id })}>{state.label}</a></h2>
            <iframe src={galleryHref(switches, { state: state.id })} title={state.label} tabIndex={-1} />
          </section>
        ))}
      </div>
    </main>
  );
}

setPlatform();
const root = document.getElementById("root");
if (root && (import.meta.env.DEV || import.meta.env.MODE === "evidence")) {
  const switches = gallerySwitches(window.location.search);
  const id = new URLSearchParams(window.location.search).get("state");
  const state = galleryStates.find((candidate) => candidate.id === id);
  if ([...toolbarMenuGalleryStates, ...chipGroupGalleryStates, ...buttonGalleryStates].some((candidate) => candidate.id === id) && !isTauri()) {
    if (switches.theme) document.documentElement.dataset.galleryTheme = switches.theme;
    document.documentElement.dataset.textSize = switches.text === "large" ? "larger" : "normal";
    if (id === "buttons") renderButtonGallery(root);
    else if (chipGroupGalleryStates.some((candidate) => candidate.id === id)) renderChipGroupGallery(root, id!);
    else renderToolbarMenuGallery(root, id!);
  } else if (state) {
    renderGalleryState(root, withSwitches(state, switches));
  } else {
    if (switches.theme) document.documentElement.dataset.galleryTheme = switches.theme;
    createRoot(root).render(<Gallery switches={switches} />);
  }
}
