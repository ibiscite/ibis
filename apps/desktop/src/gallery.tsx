import { createRoot } from "react-dom/client";
import { isTauri } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { App, type PickerApi } from "./App";
import { galleryProvider, galleryStates, type GalleryState } from "./gallery-fixtures";
import { libraryMessages } from "./messages";
import { setPlatform } from "./nativeAppearance";
import "./styles.css";
import "./gallery.css";

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
  const alerts = [library.settings_notice, library.error ? libraryMessages.openError(library.name, library.error) : null]
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
    && (root.querySelector('.library-bar p[role="status"]')?.textContent ?? null) === library.note;
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
    if (isTauri() && window.top === window) await emitTo("main", "gallery:rendered", id);
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
    stableFrames = !failed && fontsReady && document.fonts?.status !== "loading" && pending === 0 && rendered(root, state)
      ? stableFrames + 1 : 0;
    if (stableFrames >= 2) {
      reportStage("rendered");
      void acknowledge(state.id).catch(() => console.error("Gallery render acknowledgment failed."));
      return;
    }
    frame = requestAnimationFrame(check);
  };
  frame = requestAnimationFrame(check);
  return () => { stopped = true; cancelAnimationFrame(frame); app.unmount(); };
}

/** Separate evidence/dev entry: the normal production build omits this page. */
function Gallery() {
  return (
    <main className="gallery-catalog">
      <header>
        <h1>Picker states</h1>
        <p>Synthetic references for reviewing the picker. Open a state on its own to inspect it at full size.</p>
      </header>
      <div className="gallery-grid">
        {galleryStates.map((state) => (
          <section key={state.id} aria-label={state.label}>
            <h2><a href={`./gallery.html?state=${state.id}`}>{state.label}</a></h2>
            <iframe src={`./gallery.html?state=${state.id}`} title={state.label} tabIndex={-1} />
          </section>
        ))}
      </div>
    </main>
  );
}

setPlatform();
const root = document.getElementById("root");
if (root && (import.meta.env.DEV || import.meta.env.MODE === "evidence")) {
  const id = new URLSearchParams(window.location.search).get("state");
  const state = galleryStates.find((candidate) => candidate.id === id);
  if (state) {
    renderGalleryState(root, state);
  } else {
    createRoot(root).render(<Gallery />);
  }
}
