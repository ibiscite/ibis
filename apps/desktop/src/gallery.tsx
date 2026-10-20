// First, so a ?platform= switch is in place as early as this page allows.
import { galleryProvider, galleryStates, gallerySwitches, withSwitches, type GallerySwitches, type GalleryState } from "./gallery-fixtures";
import { renderToolbarMenuGallery, toolbarMenuGalleryStates } from "./ui/ToolbarMenuGallery";
import { createRoot } from "react-dom/client";
import { isTauri } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { App, type PickerApi } from "./App";
import { libraryMessages, pickerText } from "./messages";
import { setPlatform } from "./nativeAppearance";
import { trackInputModality } from "./inputModality";
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
  const alerts = [library.settings_notice, library.error ? libraryMessages.openError(library.name, library.error, library.zotero) : null]
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
    && (root.querySelector('.notices > p[role="status"]')?.textContent ?? null) === busy
    && (root.querySelector('[aria-label="Choose library to get started"]') !== null) === (library.state === "none")
    && (root.querySelector(".copy-notice code")?.textContent ?? null) === (state.copied?.text ?? null)
    && (root.querySelector('.library-bar p[role="status"]')?.textContent ?? null) === library.note
    && (document.querySelector(".gallery-notice .copied-notice code")?.textContent ?? null) === (state.notice ?? null);
}

const pinButton = (root: HTMLElement) => root.querySelector<HTMLButtonElement>(`button[aria-label="${pickerText.pin}"]`);

function press(target: Element | null, key: string, init: KeyboardEventInit = {}): void {
  target?.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init }));
}

/** What a person would do to reach a state, one step per frame once the picker has rendered. */
function seedSteps(root: HTMLElement, state: GalleryState): (() => void)[] {
  const steps: (() => void)[] = [];
  const search = () => root.querySelector("#search");
  for (let i = 0; i < (state.tray ?? 0); i++) {
    if (i > 0) steps.push(() => press(search(), "ArrowDown"));
    steps.push(() => press(search(), "Enter", { shiftKey: true }));
  }
  if (state.pinned) steps.push(() => pinButton(root)?.click());
  if (state.keys) steps.push(() => press(root.querySelector("main"), "?"));
  if (state.menuOpen) steps.push(() => root.querySelector<HTMLButtonElement>('footer button[aria-haspopup="menu"]')?.click());
  return steps;
}

/** The seeded settings and interactions are on screen, and nothing else is. */
function seeded(root: HTMLElement, state: GalleryState): boolean {
  const html = document.documentElement;
  return html.dataset.density === (state.density ?? "comfortable")
    && html.dataset.textSize === (state.largerTextSetting ? "larger" : "normal")
    && (pinButton(root)?.getAttribute("aria-pressed") === "true") === Boolean(state.pinned)
    && (root.querySelector('[role="menu"]') !== null) === Boolean(state.menuOpen)
    && (root.querySelector('[role="dialog"]') !== null) === Boolean(state.keys)
    && root.querySelectorAll(`ul[aria-label="${pickerText.selectedReferences}"] > li`).length === (state.tray ?? 0);
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
    onLibraryChanged: (handler) => track(() => provider.api.onLibraryChanged(handler)),
    onPickerShown: (handler) => track(() => provider.api.onPickerShown(handler)),
  };
  document.documentElement.dataset.galleryTheme = state.theme ?? "light";
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
        {[...galleryStates, ...(isTauri() ? [] : toolbarMenuGalleryStates)].map((state) => (
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
trackInputModality();
const root = document.getElementById("root");
if (root && (import.meta.env.DEV || import.meta.env.MODE === "evidence")) {
  const switches = gallerySwitches(window.location.search);
  const id = new URLSearchParams(window.location.search).get("state");
  const state = galleryStates.find((candidate) => candidate.id === id);
  if (toolbarMenuGalleryStates.some((candidate) => candidate.id === id) && !isTauri()) {
    if (switches.theme) document.documentElement.dataset.galleryTheme = switches.theme;
    document.documentElement.dataset.textSize = switches.text === "large" ? "larger" : "normal";
    renderToolbarMenuGallery(root, id!);
  } else if (state) {
    renderGalleryState(root, withSwitches(state, switches));
  } else {
    if (switches.theme) document.documentElement.dataset.galleryTheme = switches.theme;
    createRoot(root).render(<Gallery switches={switches} />);
  }
}
